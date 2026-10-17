package android.hardware.biometrics.common;

@VintfStability
parcelable OperationContext {
    int id;
    byte reason;
    byte order;
}
