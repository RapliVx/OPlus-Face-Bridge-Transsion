package android.hardware.keymaster;

import android.hardware.keymaster.Timestamp;

@VintfStability
parcelable HardwareAuthToken {
    long challenge;
    long userId;
    long authenticatorId;
    int authenticatorType;
    Timestamp timestamp;
    byte[] mac;
}
