package android.hardware.common;

@VintfStability
parcelable NativeHandle {
    int[] fds;
    int[] ints;
}
