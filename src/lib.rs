#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/face_aidl.rs"));

pub mod hal;
pub mod service;

pub const SERVICE_NAME: &str = "android.hardware.biometrics.face.IFace/default";
pub const INTERFACE_HASH: &str = "fca1ab84dda6c013b251270d848eb6d964a6d765";
pub const INTERFACE_VERSION: i32 = 3;
pub const COMMON_INTERFACE_HASH: &str = "a9ebb97f5abea1dc6800b69d821ef61944e80e65";
pub const COMMON_INTERFACE_VERSION: i32 = 3;
