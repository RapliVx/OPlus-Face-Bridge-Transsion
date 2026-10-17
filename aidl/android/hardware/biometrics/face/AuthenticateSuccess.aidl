package android.hardware.biometrics.face;

import android.hardware.keymaster.HardwareAuthToken;

@VintfStability
parcelable AuthenticateSuccess {
    int faceId;
    HardwareAuthToken hardwareAuthToken;
}
