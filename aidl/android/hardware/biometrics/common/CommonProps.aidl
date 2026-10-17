package android.hardware.biometrics.common;

import android.hardware.biometrics.common.ComponentInfo;

@VintfStability
parcelable CommonProps {
    int sensorId;
    byte sensorStrength;
    int maxEnrollmentsPerUser;
    ComponentInfo[] componentInfo;
}
