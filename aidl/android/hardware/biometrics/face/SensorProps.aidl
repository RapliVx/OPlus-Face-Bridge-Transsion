package android.hardware.biometrics.face;

import android.hardware.biometrics.common.CommonProps;

@VintfStability
parcelable SensorProps {
    CommonProps commonProps;
    byte sensorType;
    boolean halControlsPreview;
    int previewDisplayId;
    int enrollPreviewWidth;
    int enrollPreviewHeight;
    float enrollTranslationX;
    float enrollTranslationY;
    float enrollPreviewScale;
    boolean supportsDetectInteraction;
}
