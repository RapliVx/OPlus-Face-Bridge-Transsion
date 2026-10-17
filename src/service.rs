use std::sync::Arc;
use rsbinder::Strong;

use crate::android::hardware::biometrics::common::CommonProps::CommonProps;
use crate::android::hardware::biometrics::common::ICancellationSignal::{
    BnCancellationSignal, ICancellationSignal,
};
use crate::android::hardware::biometrics::common::OperationContext::OperationContext;
use crate::android::hardware::biometrics::face::EnrollmentStageConfig::EnrollmentStageConfig;
use crate::android::hardware::biometrics::face::FaceEnrollOptions::FaceEnrollOptions;
use crate::android::hardware::biometrics::face::IFace::{BnFace, IFace};
use crate::android::hardware::biometrics::face::ISession::{BnSession, ISession};
use crate::android::hardware::biometrics::face::ISessionCallback::ISessionCallback;
use crate::android::hardware::biometrics::face::SensorProps::SensorProps;
use crate::android::hardware::common::NativeHandle::NativeHandle;
use crate::android::hardware::keymaster::HardwareAuthToken::HardwareAuthToken;
use crate::hal::{hw_auth_token_t, FaceHal};
use crate::{COMMON_INTERFACE_HASH, COMMON_INTERFACE_VERSION, INTERFACE_HASH, INTERFACE_VERSION};

pub struct CancellationSignalService {
    hal: Arc<FaceHal>,
}

impl CancellationSignalService {
    pub fn new(hal: Arc<FaceHal>) -> Self {
        Self { hal }
    }
}

impl rsbinder::Interface for CancellationSignalService {}

impl ICancellationSignal for CancellationSignalService {
    fn r#cancel(&self) -> rsbinder::BinderResult<()> {
        tracing::info!("ICancellationSignal::cancel invoked");
        self.hal.cancel();
        Ok(())
    }

    fn r#getInterfaceVersion(&self) -> rsbinder::BinderResult<i32> {
        Ok(COMMON_INTERFACE_VERSION)
    }

    fn r#getInterfaceHash(&self) -> rsbinder::BinderResult<String> {
        Ok(COMMON_INTERFACE_HASH.to_string())
    }
}

pub struct SessionService {
    #[allow(dead_code)]
    sensor_id: i32,
    user_id: i32,
    cb: Strong<dyn ISessionCallback>,
    hal: Arc<FaceHal>,
}

impl SessionService {
    pub fn new(
        sensor_id: i32,
        user_id: i32,
        cb: Strong<dyn ISessionCallback>,
        hal: Arc<FaceHal>,
    ) -> Self {
        Self {
            sensor_id,
            user_id,
            cb,
            hal,
        }
    }
}

impl rsbinder::Interface for SessionService {}

impl ISession for SessionService {
    fn r#generateChallenge(&self) -> rsbinder::BinderResult<()> {
        tracing::info!("ISession::generateChallenge called");
        match self.hal.pre_enroll() {
            Ok(challenge) => {
                let _ = self.cb.onChallengeGenerated(challenge as i64);
                Ok(())
            }
            Err(e) => {
                tracing::error!("generateChallenge failed: {e}");
                let _ = self.cb.onChallengeGenerated(0);
                Ok(())
            }
        }
    }

    fn r#revokeChallenge(&self, challenge: i64) -> rsbinder::BinderResult<()> {
        tracing::info!("ISession::revokeChallenge called (challenge=0x{:x})", challenge);
        self.hal.post_enroll();
        let _ = self.cb.onChallengeRevoked(challenge);
        Ok(())
    }

    fn r#getEnrollmentConfig(&self, enrollment_type: i8) -> rsbinder::BinderResult<Vec<EnrollmentStageConfig>> {
        tracing::info!("ISession::getEnrollmentConfig called (type={})", enrollment_type);
        Ok(vec![])
    }

    fn r#enroll(
        &self,
        hat: &HardwareAuthToken,
        enroll_type: i8,
        _disabled_features: &[u8],
        preview_surface: &NativeHandle,
    ) -> rsbinder::BinderResult<Strong<dyn ICancellationSignal>> {
        tracing::info!(
            "ISession::enroll called (type={}, preview_surface fds={}, ints={:?})",
            enroll_type,
            preview_surface.fds.len(),
            preview_surface.ints
        );

        // Attach preview surface if token is provided
        if !preview_surface.ints.is_empty() {
            let token_byte_len = preview_surface.ints[0] as usize;
            let total_bytes_avail = (preview_surface.ints.len() - 1) * 4;
            let actual_len = token_byte_len.min(total_bytes_avail);
            if actual_len > 0 {
                let ints_slice = &preview_surface.ints[1..];
                let byte_ptr = ints_slice.as_ptr() as *const u8;
                let token_bytes = unsafe { std::slice::from_raw_parts(byte_ptr, actual_len) };
                tracing::info!(
                    "Attaching preview surface with token ({} bytes): {:02x?}",
                    actual_len, token_bytes
                );
                self.hal.set_preview_surface(token_bytes);
            }
        }

        self.hal.start_enroll_camera();
        let raw_hat = hw_auth_token_t::from_aidl(hat);
        let ret = self.hal.enroll(&raw_hat, 60, &[]);
        if ret != 0 {
            tracing::warn!("hal.enroll returned {ret}");
            self.hal.stop_enroll_camera();
        }
        let cancel = BnCancellationSignal::new_binder(CancellationSignalService::new(self.hal.clone()));
        Ok(cancel)
    }

    fn r#authenticate(&self, operation_id: i64) -> rsbinder::BinderResult<Strong<dyn ICancellationSignal>> {
        tracing::info!("ISession::authenticate called (op_id=0x{:x})", operation_id);
        let ret = self.hal.authenticate(operation_id as u64);
        if ret != 0 {
            tracing::warn!("hal.authenticate returned {ret}");
        }
        let cancel = BnCancellationSignal::new_binder(CancellationSignalService::new(self.hal.clone()));
        Ok(cancel)
    }

    fn r#detectInteraction(&self) -> rsbinder::BinderResult<Strong<dyn ICancellationSignal>> {
        tracing::info!("ISession::detectInteraction called");
        let cancel = BnCancellationSignal::new_binder(CancellationSignalService::new(self.hal.clone()));
        Ok(cancel)
    }

    fn r#enumerateEnrollments(&self) -> rsbinder::BinderResult<()> {
        tracing::info!("ISession::enumerateEnrollments called");
        let template_path = "/data/vendor_de/0/facedata/template_file.data";
        let has_template = std::path::Path::new(template_path).exists()
            && std::fs::metadata(template_path).map(|m| m.len()).unwrap_or(0) > 0;

        if !has_template {
            tracing::info!("No template file found at {template_path}, reporting 0 enrollments");
            let _ = self.cb.onEnrollmentsEnumerated(&[]);
            return Ok(());
        }

        let ret = self.hal.enumerate();
        if ret != 0 {
            tracing::warn!("hal.enumerate returned {ret}");
            let _ = self.cb.onEnrollmentsEnumerated(&[]);
        }
        Ok(())
    }

    fn r#removeEnrollments(&self, enrollment_ids: &[i32]) -> rsbinder::BinderResult<()> {
        tracing::info!("ISession::removeEnrollments called with ids {:?}", enrollment_ids);
        let template_path = "/data/vendor_de/0/facedata/template_file.data";
        let has_template = std::path::Path::new(template_path).exists()
            && std::fs::metadata(template_path).map(|m| m.len()).unwrap_or(0) > 0;

        if !has_template {
            tracing::info!("No template file found, reporting enrollments removed directly");
            let _ = self.cb.onEnrollmentsRemoved(enrollment_ids);
            return Ok(());
        }

        if enrollment_ids.is_empty() {
            let _ = self.hal.remove(0);
        } else {
            for &id in enrollment_ids {
                let _ = self.hal.remove(id as u32);
            }
        }
        Ok(())
    }

    fn r#getFeatures(&self) -> rsbinder::BinderResult<()> {
        tracing::info!("ISession::getFeatures called");
        let _ = self.cb.onFeaturesRetrieved(&[]);
        Ok(())
    }

    fn r#setFeature(&self, _hat: &HardwareAuthToken, feature: i8, _enabled: bool) -> rsbinder::BinderResult<()> {
        tracing::info!("ISession::setFeature called (feature={})", feature);
        let _ = self.cb.onFeatureSet(feature);
        Ok(())
    }

    fn r#getAuthenticatorId(&self) -> rsbinder::BinderResult<()> {
        tracing::info!("ISession::getAuthenticatorId called");
        let auth_id = self.hal.get_authenticator_id();
        let _ = self.cb.onAuthenticatorIdRetrieved(auth_id as i64);
        Ok(())
    }

    fn r#invalidateAuthenticatorId(&self) -> rsbinder::BinderResult<()> {
        tracing::info!("ISession::invalidateAuthenticatorId called");
        let auth_id = self.hal.get_authenticator_id();
        let _ = self.cb.onAuthenticatorIdInvalidated(auth_id as i64);
        Ok(())
    }

    fn r#resetLockout(&self, hat: &HardwareAuthToken) -> rsbinder::BinderResult<()> {
        tracing::info!("ISession::resetLockout called");
        let raw_hat = hw_auth_token_t::from_aidl(hat);
        let _ = self.hal.reset_lockout(&raw_hat);
        let _ = self.cb.onLockoutCleared();
        Ok(())
    }

    fn r#close(&self) -> rsbinder::BinderResult<()> {
        tracing::info!("ISession::close called for user {}", self.user_id);
        self.hal.cancel();
        self.hal.set_callback(None);
        let _ = self.cb.onSessionClosed();
        Ok(())
    }

    fn r#authenticateWithContext(
        &self,
        operation_id: i64,
        _context: &OperationContext,
    ) -> rsbinder::BinderResult<Strong<dyn ICancellationSignal>> {
        self.authenticate(operation_id)
    }

    fn r#enrollWithContext(
        &self,
        hat: &HardwareAuthToken,
        enroll_type: i8,
        disabled_features: &[u8],
        preview_surface: &NativeHandle,
        _context: &OperationContext,
    ) -> rsbinder::BinderResult<Strong<dyn ICancellationSignal>> {
        self.enroll(hat, enroll_type, disabled_features, preview_surface)
    }

    fn r#detectInteractionWithContext(
        &self,
        _context: &OperationContext,
    ) -> rsbinder::BinderResult<Strong<dyn ICancellationSignal>> {
        self.detectInteraction()
    }

    fn r#onContextChanged(&self, _context: &OperationContext) -> rsbinder::BinderResult<()> {
        Ok(())
    }

    fn r#enrollWithOptions(
        &self,
        options: &FaceEnrollOptions,
    ) -> rsbinder::BinderResult<Strong<dyn ICancellationSignal>> {
        tracing::info!(
            "ISession::enrollWithOptions called (type={}, previewSurface fds={}, ints={:?})",
            options.enrollmentType,
            options.previewSurface.fds.len(),
            options.previewSurface.ints
        );
        // Attach preview surface if token is provided
        if !options.previewSurface.ints.is_empty() {
            let token_byte_len = options.previewSurface.ints[0] as usize;
            let total_bytes_avail = (options.previewSurface.ints.len() - 1) * 4;
            let actual_len = token_byte_len.min(total_bytes_avail);
            if actual_len > 0 {
                let ints_slice = &options.previewSurface.ints[1..];
                let byte_ptr = ints_slice.as_ptr() as *const u8;
                let token_bytes = unsafe { std::slice::from_raw_parts(byte_ptr, actual_len) };
                tracing::info!(
                    "Attaching preview surface with token ({} bytes): {:02x?}",
                    actual_len, token_bytes
                );
                self.hal.set_preview_surface(token_bytes);
            }
        }

        self.hal.start_enroll_camera();
        let raw_hat = hw_auth_token_t::from_aidl(&options.hardwareAuthToken);
        let ret = self.hal.enroll(&raw_hat, 60, &[]);
        if ret != 0 {
            tracing::warn!("hal.enroll returned {ret}");
            self.hal.stop_enroll_camera();
        }
        let cancel = BnCancellationSignal::new_binder(CancellationSignalService::new(self.hal.clone()));
        Ok(cancel)
    }

    fn r#getInterfaceVersion(&self) -> rsbinder::BinderResult<i32> {
        Ok(INTERFACE_VERSION)
    }

    fn r#getInterfaceHash(&self) -> rsbinder::BinderResult<String> {
        Ok(INTERFACE_HASH.to_string())
    }
}

pub struct FaceService;

impl FaceService {
    pub fn new() -> Self {
        Self
    }

    pub fn create_binder() -> Strong<dyn IFace> {
        BnFace::new_binder(Self::new())
    }
}

impl rsbinder::Interface for FaceService {}

impl IFace for FaceService {
    fn r#getSensorProps(&self) -> rsbinder::BinderResult<Vec<SensorProps>> {
        tracing::info!("IFace::getSensorProps called");
        let props = vec![SensorProps {
            commonProps: CommonProps {
                sensorId: 1, // Face sensorId must not collide with Fingerprint (0)
                sensorStrength: 0, // SensorStrength::CONVENIENCE
                maxEnrollmentsPerUser: 5,
                componentInfo: vec![],
            },
            sensorType: 0, // FaceSensorType::RGB
            halControlsPreview: true,
            previewDisplayId: 0,
            enrollPreviewWidth: 1080,
            enrollPreviewHeight: 1920,
            enrollTranslationX: 0.0,
            enrollTranslationY: 0.0,
            enrollPreviewScale: 1.0,
            supportsDetectInteraction: false,
        }];
        Ok(props)
    }

    fn r#createSession(
        &self,
        sensor_id: i32,
        user_id: i32,
        cb: &Strong<dyn ISessionCallback>,
    ) -> rsbinder::BinderResult<Strong<dyn ISession>> {
        tracing::info!("IFace::createSession called (sensorId={}, userId={})", sensor_id, user_id);
        
        let hal = match FaceHal::get_instance() {
            Ok(h) => h,
            Err(e) => {
                tracing::error!("Failed to obtain FaceHal instance in createSession: {e}");
                return Err(rsbinder::Status::from(rsbinder::StatusCode::Unknown));
            }
        };

        let store_path = "/data/vendor_de/0/facedata";
        let _ = std::fs::create_dir_all(store_path);
        hal.set_callback(Some(cb.clone()));
        hal.set_active_user(user_id, store_path);

        let session = SessionService::new(sensor_id, user_id, cb.clone(), hal);
        Ok(BnSession::new_binder(session))
    }

    fn r#getInterfaceVersion(&self) -> rsbinder::BinderResult<i32> {
        Ok(INTERFACE_VERSION)
    }

    fn r#getInterfaceHash(&self) -> rsbinder::BinderResult<String> {
        Ok(INTERFACE_HASH.to_string())
    }
}
