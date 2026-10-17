#![allow(non_camel_case_types)]

use std::ffi::{CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use parking_lot::Mutex;
use rsbinder::Strong;

use crate::android::hardware::biometrics::face::ISessionCallback::ISessionCallback;
use crate::android::hardware::keymaster::HardwareAuthToken::HardwareAuthToken;
use crate::android::hardware::keymaster::Timestamp::Timestamp;

// Constants from Android libhardware & Face HAL
pub const HARDWARE_MODULE_TAG: u32 = 0x48574D54; // 'HWMT'
pub const HARDWARE_DEVICE_TAG: u32 = 0x48574454; // 'HWDT'

pub const FACE_MSG_ERROR: i32 = -1;
pub const FACE_MSG_ACQUIRED: i32 = 1;
pub const FACE_MSG_TEMPLATE_REMOVED: i32 = 2;
pub const FACE_MSG_TEMPLATE_ENROLLING: i32 = 3;
pub const FACE_MSG_AUTHENTICATED: i32 = 4;
pub const FACE_MSG_TEMPLATE_ENUMERATING: i32 = 5;

// Error codes
pub const FACE_ERROR_HW_UNAVAILABLE: i32 = 1;
pub const FACE_ERROR_UNABLE_TO_PROCESS: i32 = 2;
pub const FACE_ERROR_TIMEOUT: i32 = 3;
pub const FACE_ERROR_NO_SPACE: i32 = 4;
pub const FACE_ERROR_CANCELED: i32 = 5;
pub const FACE_ERROR_UNABLE_TO_REMOVE: i32 = 6;
pub const FACE_ERROR_LOCKOUT: i32 = 7;
pub const FACE_ERROR_LOCKOUT_PERMANENT: i32 = 8;

#[repr(C)]
pub struct hw_module_t {
    pub tag: u32,
    pub module_api_version: u16,
    pub hal_api_version: u16,
    pub id: *const libc::c_char,
    pub name: *const libc::c_char,
    pub author: *const libc::c_char,
    pub methods: *const hw_module_methods_t,
    pub dso: *mut libc::c_void,
    pub reserved: [u64; 32],
}

#[repr(C)]
pub struct hw_module_methods_t {
    pub open: Option<
        unsafe extern "C" fn(
            module: *const hw_module_t,
            id: *const libc::c_char,
            device: *mut *mut hw_device_t,
        ) -> libc::c_int,
    >,
}

#[repr(C)]
pub struct hw_device_t {
    pub tag: u32,
    pub version: u32,
    pub module: *const hw_module_t,
    pub reserved: [u64; 12],
    pub close: Option<unsafe extern "C" fn(device: *mut hw_device_t) -> libc::c_int>,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Default, Debug)]
pub struct hw_auth_token_t {
    pub version: u8,
    pub challenge: u64,
    pub user_id: u64,
    pub authenticator_id: u64,
    pub authenticator_type: u32,
    pub timestamp: u64,
    pub hmac: [u8; 32],
}

impl hw_auth_token_t {
    pub fn from_aidl(hat: &HardwareAuthToken) -> Self {
        let mut raw = Self::default();
        raw.version = 0;
        raw.challenge = hat.challenge as u64;
        raw.user_id = hat.userId as u64;
        raw.authenticator_id = hat.authenticatorId as u64;
        raw.authenticator_type = hat.authenticatorType as u32;
        raw.timestamp = hat.timestamp.milliSeconds as u64;
        let len = hat.mac.len().min(32);
        raw.hmac[..len].copy_from_slice(&hat.mac[..len]);
        raw
    }
}

#[repr(C)]
pub struct face_msg_t {
    pub r#type: i32,
    pub reserved: i32,
    pub face_id_or_error: i32,
    pub extra: [u8; 80],
}

pub type face_notify_t = unsafe extern "C" fn(msg: *const face_msg_t);

#[repr(C)]
pub struct face_device_t {
    pub common: hw_device_t,
    pub reserved_78: u64,
    pub notify: Option<face_notify_t>,
    pub set_notify: Option<unsafe extern "C" fn(dev: *mut face_device_t, notify: face_notify_t) -> libc::c_int>,
    pub pre_enroll: Option<unsafe extern "C" fn(dev: *mut face_device_t, timeout_sec: u32, challenge: *mut u64) -> libc::c_int>,
    pub enroll: Option<unsafe extern "C" fn(dev: *mut face_device_t, hat: *const hw_auth_token_t, timeout_sec: u32, disabled_features: *const i32, count: usize) -> libc::c_int>,
    pub post_enroll: Option<unsafe extern "C" fn(dev: *mut face_device_t) -> libc::c_int>,
    pub set_feature: Option<unsafe extern "C" fn(dev: *mut face_device_t, feature: i32, enabled: bool, hat: *const hw_auth_token_t, face_id: i32) -> libc::c_int>,
    pub get_feature: Option<unsafe extern "C" fn(dev: *mut face_device_t, feature: i32, face_id: i32, result: *mut bool) -> libc::c_int>,
    pub get_authenticator_id: Option<unsafe extern "C" fn(dev: *mut face_device_t, auth_id: *mut u64) -> libc::c_int>,
    pub cancel: Option<unsafe extern "C" fn(dev: *mut face_device_t) -> libc::c_int>,
    pub enumerate: Option<unsafe extern "C" fn(dev: *mut face_device_t) -> libc::c_int>,
    pub remove: Option<unsafe extern "C" fn(dev: *mut face_device_t, face_id: u32) -> libc::c_int>,
    pub set_active_group: Option<unsafe extern "C" fn(dev: *mut face_device_t, user_id: u32, store_path: *const libc::c_char) -> libc::c_int>,
    pub authenticate: Option<unsafe extern "C" fn(dev: *mut face_device_t, operation_id: u64) -> libc::c_int>,
    pub reserved_e8: u64,
    pub reset_lockout: Option<unsafe extern "C" fn(dev: *mut face_device_t, hat: *const hw_auth_token_t) -> libc::c_int>,
}

extern "C" {
    fn surface_bridge_create(token_data: *const u8, token_len: libc::size_t) -> *mut libc::c_void;
    fn surface_bridge_render_frame(
        bridge_ctx: *mut libc::c_void,
        nv21_data: *const u8,
        width: libc::c_int,
        height: libc::c_int,
    ) -> libc::c_int;
    fn surface_bridge_destroy(bridge_ctx: *mut libc::c_void);
    fn sensetime_set_orientation(orientation: libc::c_int) -> libc::c_int;
}

pub struct FaceHal {
    device: *mut face_device_t,
    current_callback: Arc<Mutex<Option<Strong<dyn ISessionCallback>>>>,
    is_active: AtomicBool,
    lib_base: usize,
    camera_active: AtomicBool,
    surface_bridge_ctx: Mutex<Option<usize>>,
}

unsafe impl Send for FaceHal {}
unsafe impl Sync for FaceHal {}

static GLOBAL_HAL: OnceLock<Arc<FaceHal>> = OnceLock::new();
static INIT_MUTEX: Mutex<()> = Mutex::new(());
static REAL_OPEN_CAMERA: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

unsafe extern "C" fn hooked_open_camera(
    mgr: *mut libc::c_void,
    camera_id: *const libc::c_char,
    callback: *mut libc::c_void,
    device: *mut *mut libc::c_void,
) -> libc::c_int {
    let orig_id = if !camera_id.is_null() {
        CStr::from_ptr(camera_id).to_string_lossy().into_owned()
    } else {
        "null".to_string()
    };
    tracing::info!("hooked ACameraManager_openCamera intercepted call for camera '{}'", orig_id);

    let front_id = CString::new("1").unwrap();
    let real_fn_ptr = REAL_OPEN_CAMERA.load(Ordering::SeqCst);
    if real_fn_ptr == 0 {
        tracing::error!("REAL_OPEN_CAMERA is not set!");
        return -1;
    }
    let real_fn: unsafe extern "C" fn(
        *mut libc::c_void,
        *const libc::c_char,
        *mut libc::c_void,
        *mut *mut libc::c_void,
    ) -> libc::c_int = std::mem::transmute(real_fn_ptr);

    let ret = real_fn(mgr, front_id.as_ptr(), callback, device);
    tracing::info!("Real ACameraManager_openCamera for id '1' returned {}", ret);
    ret
}

struct FeedQueue {
    frame_data: Mutex<Option<(Vec<u8>, i32)>>,
    condvar: parking_lot::Condvar,
}

static FEED_QUEUE: OnceLock<FeedQueue> = OnceLock::new();

fn get_feed_queue() -> &'static FeedQueue {
    FEED_QUEUE.get_or_init(|| {
        let q = FeedQueue {
            frame_data: Mutex::new(None),
            condvar: parking_lot::Condvar::new(),
        };
        std::thread::Builder::new()
            .name("face_yuv_feeder".to_string())
            .spawn(|| {
                yuv_feeder_worker();
            })
            .expect("failed to spawn yuv feeder thread");
        q
    })
}

fn yuv_feeder_worker() {
    let queue = get_feed_queue();
    let path_tmp = "/data/vendor_de/0/facedata/faceIdEnroll.tmp";
    let path_yuv = "/data/vendor_de/0/facedata/faceIdEnroll.yuv";
    let mut last_write = std::time::Instant::now();

    loop {
        let frame = {
            let mut lock = queue.frame_data.lock();
            while lock.is_none() {
                queue.condvar.wait(&mut lock);
            }
            lock.take()
        };

        if let Some((data, frame_index)) = frame {
            if let Some(hal) = GLOBAL_HAL.get() {
                if !hal.camera_active.load(Ordering::SeqCst) {
                    continue;
                }
            } else {
                continue;
            }

            // Check if faceIdEnroll.yuv was consumed by SenseTime (< 100 bytes)
            let is_empty = std::fs::metadata(path_yuv).map(|m| m.len() < 100).unwrap_or(true);
            let now = std::time::Instant::now();
            if !is_empty && now.duration_since(last_write) < std::time::Duration::from_millis(150) {
                // SenseTime is still processing previous frame; skip writing to avoid contention
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue;
            }

            // Always ensure SenseTime orientation is 8 (270 degrees) before feeding frame
            let _ = unsafe { sensetime_set_orientation(8) };

            let _ = std::fs::create_dir_all("/data/vendor_de/0/facedata");
            if std::fs::write(path_tmp, &data).is_ok() {
                let _ = std::fs::rename(path_tmp, path_yuv);
                last_write = now;
                if frame_index == 1 || frame_index % 30 == 0 {
                    tracing::info!("Async fed camera frame {} ({} bytes) to faceIdEnroll.yuv", frame_index, data.len());
                }
            }
        }
    }
}

unsafe extern "C" fn enroll_preview_cb(data: *const u8, frame_index: i32) {
    if data.is_null() {
        return;
    }
    if frame_index == 1 || frame_index % 60 == 0 {
        tracing::info!("Camera preview callback received frame {}", frame_index);
    }
    if let Some(hal) = GLOBAL_HAL.get() {
        if !hal.camera_active.load(Ordering::SeqCst) {
            return;
        }
        // Render frame to ColorOS preview surface (if bridge is active)
        hal.render_preview(data, 640, 480);
    }

    let yuv_len = 640 * 480 * 3 / 2; // 460,800 bytes (NV21 640x480)
    let yuv_slice = std::slice::from_raw_parts(data, yuv_len);

    let queue = get_feed_queue();
    {
        let mut lock = queue.frame_data.lock();
        if let Some((ref mut buf, ref mut idx)) = *lock {
            if buf.len() == yuv_len {
                buf.copy_from_slice(yuv_slice);
            } else {
                *buf = yuv_slice.to_vec();
            }
            *idx = frame_index;
        } else {
            *lock = Some((yuv_slice.to_vec(), frame_index));
        }
        queue.condvar.notify_one();
    }
}

unsafe extern "C" fn enroll_first_frame_cb() {
    tracing::info!("Front camera preview produced first frame");
}

unsafe extern "C" fn hal_notify_trampoline(msg: *const face_msg_t) {
    if msg.is_null() {
        return;
    }
    let msg_ref = &*msg;
    if let Some(hal) = GLOBAL_HAL.get() {
        hal.on_notify(msg_ref);
    }
}

impl FaceHal {
    pub fn get_instance() -> anyhow::Result<Arc<FaceHal>> {
        if let Some(instance) = GLOBAL_HAL.get() {
            return Ok(instance.clone());
        }

        let _guard = INIT_MUTEX.lock();
        if let Some(instance) = GLOBAL_HAL.get() {
            return Ok(instance.clone());
        }

        Self::open()
    }

    fn open() -> anyhow::Result<Arc<Self>> {
        let candidate_libs = [
            "/vendor/lib64/hw/face.default.so",
            "/vendor/lib64/hw/face.mt6789.so",
            "/odm/lib64/hw/face.default.so",
            "/system/lib64/hw/face.default.so",
        ];

        let mut handle: *mut libc::c_void = std::ptr::null_mut();
        let mut loaded_path = "";

        for path in candidate_libs {
            let c_path = CString::new(path).unwrap();
            handle = unsafe { libc::dlopen(c_path.as_ptr(), libc::RTLD_NOW) };
            if !handle.is_null() {
                loaded_path = path;
                tracing::info!("Successfully loaded face HAL from {}", path);
                break;
            }
        }

        if handle.is_null() {
            let err = unsafe {
                let err_ptr = libc::dlerror();
                if !err_ptr.is_null() {
                    CStr::from_ptr(err_ptr).to_string_lossy().into_owned()
                } else {
                    "unknown dlopen error".to_string()
                }
            };
            anyhow::bail!("failed to dlopen face HAL: {err}");
        }

        let hmi_sym = CString::new("HMI").unwrap();
        let hmi_ptr = unsafe { libc::dlsym(handle, hmi_sym.as_ptr()) as *const hw_module_t };
        if hmi_ptr.is_null() {
            anyhow::bail!("failed to find symbol 'HMI' in {loaded_path}");
        }

        let module = unsafe { &*hmi_ptr };
        if module.tag != HARDWARE_MODULE_TAG {
            anyhow::bail!("invalid module tag: 0x{:x} (expected 0x{:x})", module.tag, HARDWARE_MODULE_TAG);
        }

        let methods = unsafe { module.methods.as_ref() }
            .ok_or_else(|| anyhow::anyhow!("module methods pointer is null"))?;

        let open_fn = methods.open.ok_or_else(|| anyhow::anyhow!("open function pointer is null"))?;

        let lib_base = (hmi_ptr as usize).saturating_sub(0xfaa0);
        tracing::info!("Calculated face HAL lib_base at 0x{:x} (from HMI 0x{:x})", lib_base, hmi_ptr as usize);

        // Bypass SenseTime proprietary license check before calling face_open
        Self::patch_sensetime_license(handle, loaded_path, lib_base);

        // Fix Transsion "1-faceid" / "2-faceid" camera ID to standard "1"
        Self::patch_camera_id(handle, lib_base);

        // Bypass Transsion HAL challenge verification to prevent error 1004
        Self::patch_challenge_check(lib_base);

        let mut dev_ptr: *mut hw_device_t = std::ptr::null_mut();
        let id_face = CString::new("face").unwrap();
        let ret = unsafe { open_fn(hmi_ptr, id_face.as_ptr(), &mut dev_ptr) };
        if ret != 0 || dev_ptr.is_null() {
            anyhow::bail!("failed to open face device: ret = {ret}");
        }

        let face_dev = dev_ptr as *mut face_device_t;
        let face_ref = unsafe { &mut *face_dev };

        tracing::info!("Face HAL device opened successfully (tag=0x{:x}, ver=0x{:x})", face_ref.common.tag, face_ref.common.version);

        let hal = Arc::new(Self {
            device: face_dev,
            current_callback: Arc::new(Mutex::new(None)),
            is_active: AtomicBool::new(false),
            lib_base,
            camera_active: AtomicBool::new(false),
            surface_bridge_ctx: Mutex::new(None),
        });

        // Set GLOBAL_HAL BEFORE set_notify and set_active_group
        // because set_active_group in Transsion's face.default.so synchronously invokes dev->notify!
        let _ = GLOBAL_HAL.set(hal.clone());

        // Register notify callback
        if let Some(set_notify) = face_ref.set_notify {
            let notify_ret = unsafe { set_notify(face_dev, hal_notify_trampoline) };
            tracing::info!("Registered HAL notify trampoline: ret = {notify_ret}");
        }

        // Set default active group
        if let Some(set_active_group) = face_ref.set_active_group {
            let path = "/data/vendor_de/0/facedata";
            let _ = std::fs::create_dir_all(path);
            let path_facedata = CString::new(path).unwrap();
            let ret = unsafe { set_active_group(face_dev, 0, path_facedata.as_ptr()) };
            tracing::info!("Set active group to {path}: ret = {ret}");
        }

        Ok(hal)
    }

    pub fn set_callback(&self, cb: Option<Strong<dyn ISessionCallback>>) {
        let mut lock = self.current_callback.lock();
        *lock = cb;
    }

    pub fn set_preview_surface(&self, token_bytes: &[u8]) {
        let mut lock = self.surface_bridge_ctx.lock();
        if let Some(old_ctx) = lock.take() {
            unsafe { surface_bridge_destroy(old_ctx as *mut libc::c_void) };
        }
        let ctx = unsafe { surface_bridge_create(token_bytes.as_ptr(), token_bytes.len()) };
        if !ctx.is_null() {
            tracing::info!("Preview surface bridge successfully attached: {:p}", ctx);
            *lock = Some(ctx as usize);
        } else {
            tracing::warn!("Failed to create preview surface bridge from token");
        }
    }

    pub fn render_preview(&self, nv21: *const u8, width: i32, height: i32) {
        let lock = self.surface_bridge_ctx.lock();
        if let Some(ctx) = *lock {
            unsafe {
                surface_bridge_render_frame(ctx as *mut libc::c_void, nv21, width, height);
            }
        }
    }

    pub fn clear_preview_surface(&self) {
        let mut lock = self.surface_bridge_ctx.lock();
        if let Some(ctx) = lock.take() {
            tracing::info!("Releasing preview surface bridge: 0x{:x}", ctx);
            unsafe {
                surface_bridge_destroy(ctx as *mut libc::c_void);
            }
        }
    }

    pub fn set_active_user(&self, user_id: i32, store_path: &str) -> i32 {
        if let Some(dev) = unsafe { self.device.as_ref() } {
            if let Some(set_active_group) = dev.set_active_group {
                // Ensure store directory exists
                let target_path = if store_path.is_empty() {
                    "/data/vendor_de/0/facedata"
                } else {
                    store_path
                };
                let _ = std::fs::create_dir_all(target_path);
                let c_path = CString::new(target_path).unwrap_or_else(|_| CString::new("/data/vendor_de/0/facedata").unwrap());
                
                // Transsion face.default.so strictly requires user_id == 0.
                // Any non-zero user_id triggers "face_set_active_group subuser" error and skips st_auth_init!
                let ret = unsafe { set_active_group(self.device, 0, c_path.as_ptr()) };
                tracing::info!(
                    "set_active_group(fwk_user_id={}, forced_hw_user_id=0, store_path={}): ret = {}",
                    user_id, target_path, ret
                );
                return ret;
            }
        }
        -1
    }

    pub fn pre_enroll(&self) -> anyhow::Result<u64> {
        let dev = unsafe { self.device.as_ref() }.ok_or_else(|| anyhow::anyhow!("device null"))?;
        let pre_enroll_fn = dev.pre_enroll.ok_or_else(|| anyhow::anyhow!("pre_enroll not supported"))?;

        let mut challenge: u64 = 0;
        let ret = unsafe { pre_enroll_fn(self.device, 60, &mut challenge) };
        tracing::info!("pre_enroll returned challenge: 0x{:x}, ret: {}", challenge, ret);
        Ok(challenge)
    }

    pub fn post_enroll(&self) -> i32 {
        if let Some(dev) = unsafe { self.device.as_ref() } {
            if let Some(post_enroll_fn) = dev.post_enroll {
                let ret = unsafe { post_enroll_fn(self.device) };
                tracing::info!("post_enroll ret: {}", ret);
                return ret;
            }
        }
        0
    }

    pub fn enroll(&self, hat: &hw_auth_token_t, timeout_sec: u32, disabled_features: &[i32]) -> i32 {
        self.is_active.store(true, Ordering::SeqCst);

        // Configure SenseTime face orientation to 8 (270 degrees) for MT6789 front camera
        let orient_ret = unsafe { sensetime_set_orientation(8) };
        tracing::info!("sensetime_set_orientation(8) before dev.enroll ret = {orient_ret}");

        if let Some(dev) = unsafe { self.device.as_ref() } {
            if let Some(enroll_fn) = dev.enroll {
                let feat_ptr = if disabled_features.is_empty() {
                    std::ptr::null()
                } else {
                    disabled_features.as_ptr()
                };
                let ret = unsafe { enroll_fn(self.device, hat, timeout_sec, feat_ptr, disabled_features.len()) };
                tracing::info!("enroll started: ret: {}", ret);

                // Re-apply orientation after Transsion enroll timer setup
                let _ = unsafe { sensetime_set_orientation(8) };

                return ret;
            }
        }
        -1
    }

    pub fn authenticate(&self, operation_id: u64) -> i32 {
        self.is_active.store(true, Ordering::SeqCst);
        if let Some(dev) = unsafe { self.device.as_ref() } {
            if let Some(auth_fn) = dev.authenticate {
                let ret = unsafe { auth_fn(self.device, operation_id) };
                tracing::info!("authenticate started with op_id 0x{:x}: ret: {}", operation_id, ret);
                return ret;
            }
        }
        -1
    }

    pub fn start_enroll_camera(&self) {
        if self.camera_active.swap(true, Ordering::SeqCst) {
            tracing::warn!("Camera feeder is already active");
            return;
        }

        // Enforce SenseTime orientation 8
        let _ = unsafe { sensetime_set_orientation(8) };

        tracing::info!(
            "Starting front camera feeder for face enrollment (lib_base=0x{:x}, fn=0x{:x}, ctx=0x{:x})...",
            self.lib_base,
            self.lib_base + 0xb6cc,
            self.lib_base + 0x10f38
        );
        if self.lib_base == 0 {
            tracing::error!("Cannot start camera: lib_base is 0");
            return;
        }

        // Clean up any stale YUV before starting
        let _ = std::fs::remove_file("/data/vendor_de/0/facedata/faceIdEnroll.tmp");
        let _ = std::fs::remove_file("/data/vendor_de/0/facedata/faceIdEnroll.yuv");

        let camera_ctx = (self.lib_base + 0x10f38) as *mut libc::c_void;
        let start_camera_fn: Option<
            unsafe extern "C" fn(
                *mut libc::c_void,
                i32,
                unsafe extern "C" fn(*const u8, i32),
                unsafe extern "C" fn(),
            ) -> i32,
        > = unsafe { std::mem::transmute(self.lib_base + 0xb6cc) };

        if let Some(start_fn) = start_camera_fn {
            let ret = unsafe {
                start_fn(
                    camera_ctx,
                    1000,
                    enroll_preview_cb,
                    enroll_first_frame_cb,
                )
            };
            tracing::info!("startCameraCapture returned {}", ret);
            if ret != 0 {
                self.camera_active.store(false, Ordering::SeqCst);
            }
        }
    }

    pub fn stop_enroll_camera(&self) {
        if !self.camera_active.swap(false, Ordering::SeqCst) {
            return;
        }
        self.clear_preview_surface();
        tracing::info!("Stopping front camera feeder for face enrollment...");
        if self.lib_base == 0 {
            return;
        }
        let camera_ctx = (self.lib_base + 0x10f38) as *mut libc::c_void;
        let close_camera_fn: Option<unsafe extern "C" fn(*mut libc::c_void, i32) -> i32> = unsafe {
            std::mem::transmute(self.lib_base + 0x87b4)
        };
        if let Some(close_fn) = close_camera_fn {
            let ret = unsafe { close_fn(camera_ctx, 0) };
            tracing::info!("closeCamera returned {}", ret);
        }
        // Clean up temporary files
        let _ = std::fs::remove_file("/data/vendor_de/0/facedata/faceIdEnroll.tmp");
        let _ = std::fs::remove_file("/data/vendor_de/0/facedata/faceIdEnroll.yuv");
        if let Some(queue) = FEED_QUEUE.get() {
            let mut lock = queue.frame_data.lock();
            *lock = None;
        }
    }

    pub fn cancel(&self) -> i32 {
        self.is_active.store(false, Ordering::SeqCst);
        self.stop_enroll_camera();
        if let Some(dev) = unsafe { self.device.as_ref() } {
            if let Some(cancel_fn) = dev.cancel {
                let ret = unsafe { cancel_fn(self.device) };
                tracing::info!("cancel requested: ret: {}", ret);
                return ret;
            }
        }
        0
    }

    pub fn enumerate(&self) -> i32 {
        if let Some(dev) = unsafe { self.device.as_ref() } {
            if let Some(enum_fn) = dev.enumerate {
                return unsafe { enum_fn(self.device) };
            }
        }
        0
    }

    pub fn remove(&self, face_id: u32) -> i32 {
        if let Some(dev) = unsafe { self.device.as_ref() } {
            if let Some(remove_fn) = dev.remove {
                return unsafe { remove_fn(self.device, face_id) };
            }
        }
        0
    }

    pub fn get_authenticator_id(&self) -> u64 {
        if let Some(dev) = unsafe { self.device.as_ref() } {
            if let Some(get_auth_fn) = dev.get_authenticator_id {
                let mut auth_id: u64 = 0;
                let _ = unsafe { get_auth_fn(self.device, &mut auth_id) };
                return auth_id;
            }
        }
        0
    }

    pub fn reset_lockout(&self, hat: &hw_auth_token_t) -> i32 {
        if let Some(dev) = unsafe { self.device.as_ref() } {
            if let Some(reset_fn) = dev.reset_lockout {
                return unsafe { reset_fn(self.device, hat) };
            }
        }
        0
    }

    fn on_notify(&self, msg: &face_msg_t) {
        let cb_opt = {
            let lock = self.current_callback.lock();
            lock.clone()
        };

        let cb = match cb_opt {
            Some(c) => c,
            None => {
                tracing::warn!("Received HAL notify type {} but no callback registered", msg.r#type);
                return;
            }
        };

        match msg.r#type {
            FACE_MSG_AUTHENTICATED => {
                let fid = msg.face_id_or_error;
                tracing::info!("HAL notify: FACE_AUTHENTICATED (fid={})", fid);
                self.is_active.store(false, Ordering::SeqCst);

                if fid != 0 {
                    // Extract hw_auth_token_t from msg.extra
                    let mut raw_token = hw_auth_token_t::default();
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            msg.extra.as_ptr(),
                            &mut raw_token as *mut hw_auth_token_t as *mut u8,
                            std::mem::size_of::<hw_auth_token_t>(),
                        );
                    }

                    let hat = HardwareAuthToken {
                        challenge: raw_token.challenge as i64,
                        userId: raw_token.user_id as i64,
                        authenticatorId: raw_token.authenticator_id as i64,
                        authenticatorType: raw_token.authenticator_type as i32,
                        timestamp: Timestamp {
                            milliSeconds: raw_token.timestamp as i64,
                        },
                        mac: raw_token.hmac.to_vec(),
                    };

                    let _ = cb.onAuthenticationSucceeded(fid, &hat);
                } else {
                    let _ = cb.onAuthenticationFailed();
                }
            }
            FACE_MSG_TEMPLATE_ENROLLING => {
                let fid = msg.face_id_or_error;
                let remaining = unsafe { std::ptr::read_unaligned(msg.extra.as_ptr() as *const i32) };
                tracing::info!("HAL notify: FACE_TEMPLATE_ENROLLING (fid={}, remaining={})", fid, remaining);
                if remaining == 0 {
                    self.is_active.store(false, Ordering::SeqCst);
                    self.stop_enroll_camera();
                }
                let _ = cb.onEnrollmentProgress(fid, remaining);
            }
            FACE_MSG_TEMPLATE_REMOVED => {
                let fid = msg.face_id_or_error;
                tracing::info!("HAL notify: FACE_TEMPLATE_REMOVED (fid={})", fid);
            }
            FACE_MSG_TEMPLATE_ENUMERATING => {
                let fid = msg.face_id_or_error;
                tracing::info!("HAL notify: FACE_TEMPLATE_ENUMERATING (fid={})", fid);
            }
            FACE_MSG_ERROR => {
                let err = msg.face_id_or_error;
                tracing::info!("HAL notify: FACE_ERROR (err={})", err);
                self.is_active.store(false, Ordering::SeqCst);
                self.stop_enroll_camera();
                let (error_code, vendor_code) = if err > 8 {
                    (7i8, err) // 7 = Error::VENDOR
                } else {
                    (err as i8, 0)
                };
                let _ = cb.onError(error_code, vendor_code);
            }
            FACE_MSG_ACQUIRED => {
                tracing::debug!("HAL notify: FACE_ACQUIRED (info={})", msg.face_id_or_error);
            }
            other => {
                tracing::debug!("HAL notify: unknown event type {}", other);
            }
        }
    }

    fn patch_sensetime_license(handle: *mut libc::c_void, _loaded_path: &str, lib_base: usize) {
        let sym_name = CString::new("st_face_init_license").unwrap();
        let original_fn_ptr = unsafe { libc::dlsym(handle, sym_name.as_ptr()) as usize };
        tracing::info!("Original st_face_init_license symbol address: 0x{:x}", original_fn_ptr);

        let base = lib_base;
        tracing::info!("Using face HAL base at 0x{:x}", base);

        // Primary target: known GOT offset 0xe770
        let got_entry = (base + 0xe770) as *mut usize;
        let page_size = 4096;
        let page_addr = (got_entry as usize) & !(page_size - 1);

        unsafe {
            let prot_res = libc::mprotect(
                page_addr as *mut libc::c_void,
                page_size * 2,
                libc::PROT_READ | libc::PROT_WRITE,
            );
            if prot_res == 0 {
                *got_entry = dummy_st_face_init_license as *const () as usize;
                tracing::info!(
                    "Patched face HAL GOT[0xe770] at 0x{:x} -> 0x{:x}",
                    got_entry as usize,
                    *got_entry
                );
            } else {
                tracing::warn!("mprotect failed for GOT page: {prot_res}");
            }

            // Dynamic scan over .got / .got.plt range (base + 0xe000 .. base + 0xf000)
            if original_fn_ptr != 0 {
                let start = (base + 0xe000) as *mut usize;
                let count = (0x1000) / std::mem::size_of::<usize>();
                for i in 0..count {
                    let slot = start.add(i);
                    if *slot == original_fn_ptr {
                        *slot = dummy_st_face_init_license as *const () as usize;
                        tracing::info!("Dynamically matched and patched GOT slot at 0x{:x}", slot as usize);
                    }
                }
            }
        }

        let maps = match std::fs::read_to_string("/proc/self/maps") {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to read /proc/self/maps: {e}");
                return;
            }
        };

        // Also set libstfaceunlockppl.so internal license flag
        for line in maps.lines() {
            if line.contains("libstfaceunlockppl.so") {
                if let Some(range) = line.split_whitespace().next() {
                    let parts: Vec<&str> = range.split('-').collect();
                    if let Ok(base) = usize::from_str_radix(parts[0], 16) {
                        tracing::info!("Found libstfaceunlockppl.so base at 0x{:x}", base);
                        let flag_ptr = (base + 0x33914e8) as *mut u8;
                        let page_size = 4096;
                        let page_addr = (flag_ptr as usize) & !(page_size - 1);
                        unsafe {
                            let prot_res = libc::mprotect(
                                page_addr as *mut libc::c_void,
                                page_size,
                                libc::PROT_READ | libc::PROT_WRITE,
                            );
                            if prot_res == 0 {
                                *flag_ptr = 1;
                                tracing::info!("Set libstfaceunlockppl is_licensed flag to 1 at 0x{:x}", flag_ptr as usize);
                            }

                            // Also patch SDK protector license check function at 0x30b334
                            let lic_fn_ptr = (base + 0x30b334) as *mut u32;
                            let lic_page = (lic_fn_ptr as usize) & !(page_size - 1);
                            let lic_prot = libc::mprotect(
                                lic_page as *mut libc::c_void,
                                page_size,
                                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
                            );
                            if lic_prot == 0 {
                                // mov w0, #0 (0x52800000)
                                // ret        (0xd65f03c0)
                                *lic_fn_ptr = 0x52800000;
                                *lic_fn_ptr.add(1) = 0xd65f03c0;
                                libc::mprotect(
                                    lic_page as *mut libc::c_void,
                                    page_size,
                                    libc::PROT_READ | libc::PROT_EXEC,
                                );
                                tracing::info!(
                                    "Patched libstfaceunlockppl protector at 0x{:x} -> mov w0, #0; ret",
                                    lic_fn_ptr as usize
                                );
                            } else {
                                tracing::warn!("mprotect failed for protector page: {lic_prot}");
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    fn patch_camera_id(handle: *mut libc::c_void, lib_base: usize) {
        // 1. Patch hardcoded .rodata strings in face.default.so: "1-faceid" and "2-faceid" -> "1"
        let targets = [
            0x4fb5, // "1-faceid\0"
            0x5190, // "2-faceid\0"
        ];
        let page_size = 4096;

        for offset in targets {
            let addr = (lib_base + offset) as *mut u8;
            let page_addr = (addr as usize) & !(page_size - 1);
            unsafe {
                let prot_res = libc::mprotect(
                    page_addr as *mut libc::c_void,
                    page_size,
                    libc::PROT_READ | libc::PROT_WRITE,
                );
                if prot_res == 0 {
                    std::ptr::write_bytes(addr, 0, 9);
                    *addr = b'1';
                    libc::mprotect(page_addr as *mut libc::c_void, page_size, libc::PROT_READ);
                    tracing::info!("Patched camera ID string at 0x{:x} (offset 0x{:x}) to '1'", addr as usize, offset);
                } else {
                    tracing::warn!("Failed to mprotect rodata page 0x{:x} for camera ID patch: {}", page_addr, prot_res);
                }
            }
        }

        // 2. Resolve real ACameraManager_openCamera and hook GOT[0xe9d8]
        let sym_name = CString::new("ACameraManager_openCamera").unwrap();
        let real_fn_ptr = unsafe { libc::dlsym(handle, sym_name.as_ptr()) as usize };
        if real_fn_ptr != 0 {
            REAL_OPEN_CAMERA.store(real_fn_ptr, Ordering::SeqCst);
            tracing::info!("Resolved real ACameraManager_openCamera: 0x{:x}", real_fn_ptr);
        }

        let got_entry = (lib_base + 0xe9d8) as *mut usize;
        let got_page = (got_entry as usize) & !(page_size - 1);
        unsafe {
            if libc::mprotect(got_page as *mut libc::c_void, page_size, libc::PROT_READ | libc::PROT_WRITE) == 0 {
                let current_val = *got_entry;
                if REAL_OPEN_CAMERA.load(Ordering::SeqCst) == 0 && current_val != 0 {
                    REAL_OPEN_CAMERA.store(current_val, Ordering::SeqCst);
                }
                *got_entry = hooked_open_camera as *const () as usize;
                tracing::info!(
                    "Hooked ACameraManager_openCamera GOT[0xe9d8] at 0x{:x} -> 0x{:x} (real=0x{:x})",
                    got_entry as usize,
                    hooked_open_camera as *const () as usize,
                    REAL_OPEN_CAMERA.load(Ordering::SeqCst)
                );
            } else {
                tracing::warn!("Failed to mprotect GOT page for ACameraManager_openCamera");
            }
        }
    }

    fn patch_challenge_check(lib_base: usize) {
        let offset = 0x9ae8;
        let addr = (lib_base + offset) as *mut u32;
        let page_size = 4096;
        let page_addr = (addr as usize) & !(page_size - 1);
        unsafe {
            let prot_res = libc::mprotect(
                page_addr as *mut libc::c_void,
                page_size,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
            );
            if prot_res == 0 {
                let current_inst = *addr;
                if current_inst == 0x54000621 {
                    // NOP out the b.ne 0x9bac instruction (0xd503201f)
                    *addr = 0xd503201f;
                    tracing::info!(
                        "Patched face_enroll challenge check at 0x{:x} (0x54000621 -> NOP 0xd503201f)",
                        addr as usize
                    );
                } else if current_inst == 0xd503201f {
                    tracing::info!("face_enroll challenge check already NOPed at 0x{:x}", addr as usize);
                } else {
                    tracing::warn!(
                        "Unexpected instruction at 0x{:x}: 0x{:08x} (expected 0x54000621)",
                        addr as usize, current_inst
                    );
                }
                libc::mprotect(page_addr as *mut libc::c_void, page_size, libc::PROT_READ | libc::PROT_EXEC);
            } else {
                tracing::warn!("Failed to mprotect page for challenge check patch: {prot_res}");
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn dummy_st_face_init_license(_lic: *const libc::c_char) -> libc::c_int {
    tracing::info!("st_face_init_license intercepted! Returning 0 (OK)");
    0
}

#[no_mangle]
pub unsafe extern "C" fn st_face_init_license(_lic: *const libc::c_char) -> libc::c_int {
    tracing::info!("st_face_init_license (global symbol) called! Returning 0 (OK)");
    0
}
