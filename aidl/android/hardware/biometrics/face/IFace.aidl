package android.hardware.biometrics.face;

import android.hardware.biometrics.face.ISession;
import android.hardware.biometrics.face.ISessionCallback;
import android.hardware.biometrics.face.SensorProps;

@VintfStability
interface IFace {
    SensorProps[] getSensorProps();
    ISession createSession(in int sensorId, in int userId, in ISessionCallback cb);

    int getInterfaceVersion();
    String getInterfaceHash();
}
