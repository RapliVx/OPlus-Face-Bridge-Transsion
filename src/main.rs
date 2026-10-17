#![allow(non_snake_case)]

use oplus_face_bridge_transsion::android::hardware::biometrics::face::IFace::BnFace;
use oplus_face_bridge_transsion::service::FaceService;
use oplus_face_bridge_transsion::SERVICE_NAME;

#[cfg(target_os = "android")]
extern "C" {
    fn __system_property_get(name: *const libc::c_char, value: *mut libc::c_char) -> libc::c_int;
}

#[cfg(not(target_os = "android"))]
unsafe fn __system_property_get(_name: *const libc::c_char, _value: *mut libc::c_char) -> libc::c_int {
    0
}

fn get_system_prop(key: &[u8], default: &str) -> String {
    let mut val = [0u8; 92];
    let len = unsafe {
        __system_property_get(key.as_ptr() as *const libc::c_char, val.as_mut_ptr() as *mut libc::c_char)
    };
    if len > 0 {
        String::from_utf8_lossy(&val[..len as usize]).into_owned()
    } else {
        default.to_string()
    }
}

#[cfg(target_os = "android")]
extern "C" {
    fn __android_log_print(prio: libc::c_int, tag: *const libc::c_char, fmt: *const libc::c_char, ...) -> libc::c_int;
}

#[cfg(target_os = "android")]
#[derive(Clone)]
struct AndroidLogWriter {
    tag: std::ffi::CString,
}

#[cfg(target_os = "android")]
impl std::io::Write for AndroidLogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let msg = String::from_utf8_lossy(buf);
        let trimmed = msg.trim_end_matches('\n');
        if !trimmed.is_empty() {
            let c_msg = std::ffi::CString::new(trimmed).unwrap_or_default();
            let fmt = std::ffi::CStr::from_bytes_with_nul(b"%s\0").unwrap();
            unsafe {
                __android_log_print(4, self.tag.as_ptr(), fmt.as_ptr(), c_msg.as_ptr());
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(target_os = "android")]
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for AndroidLogWriter {
    type Writer = AndroidLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn init_logging() {
    let _ = tracing_log::LogTracer::init();

    #[cfg(target_os = "android")]
    {
        use tracing_subscriber::fmt::format::Format;

        let writer = AndroidLogWriter {
            tag: std::ffi::CString::new("face_transsion").unwrap(),
        };

        let _ = tracing_subscriber::fmt()
            .event_format(Format::default().without_time())
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .with_writer(writer)
            .with_ansi(false)
            .try_init();
    }

    #[cfg(not(target_os = "android"))]
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

fn main() -> anyhow::Result<()> {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }

    init_logging();

    let brand = get_system_prop(b"ro.product.brand\0", "Transsion");
    let model = get_system_prop(b"ro.product.model\0", "Device");
    let platform = get_system_prop(b"ro.board.platform\0", "Universal");

    tracing::info!(
        "Starting android.hardware.biometrics.face-service for Transsion ({} {}, SoC: {})...",
        brand, model, platform
    );

    // Initialize Binder process state FIRST
    rsbinder::ProcessState::init_default().map_err(|error| {
        anyhow::anyhow!("failed to initialize Binder process state: {error}")
    })?;
    rsbinder::ProcessState::start_thread_pool();

    // Register AIDL face service IMMEDIATELY so ServiceManager & system_server never wait/deadlock
    let service = FaceService::new();
    let binder = BnFace::new_binder(service);
    rsbinder::hub::add_service(SERVICE_NAME, binder.as_binder())?;

    tracing::info!(
        "Successfully registered {} with ServiceManager! Waiting for transactions...",
        SERVICE_NAME
    );

    // Hardware HAL and SenseTime engine will be lazily loaded on first createSession() call
    rsbinder::ProcessState::join_thread_pool()?;
    Ok(())
}
