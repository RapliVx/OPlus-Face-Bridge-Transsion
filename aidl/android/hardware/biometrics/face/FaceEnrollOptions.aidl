package android.hardware.biometrics.face;

import android.hardware.biometrics.common.OperationContext;
import android.hardware.common.NativeHandle;
import android.hardware.keymaster.HardwareAuthToken;

@VintfStability
parcelable FaceEnrollOptions {
    HardwareAuthToken hardwareAuthToken;
    byte enrollmentType;
    byte[] features;
    NativeHandle previewSurface;
    OperationContext context;
}
