package android.hardware.biometrics.common;

@VintfStability
parcelable ComponentInfo {
    String componentId;
    String hardwareVersion;
    String firmwareVersion;
    String serialNumber;
    String softwareVersion;
}
