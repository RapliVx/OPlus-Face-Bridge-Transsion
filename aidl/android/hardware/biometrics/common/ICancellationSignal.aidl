package android.hardware.biometrics.common;

@VintfStability
interface ICancellationSignal {
    oneway void cancel();

    int getInterfaceVersion();
    String getInterfaceHash();
}
