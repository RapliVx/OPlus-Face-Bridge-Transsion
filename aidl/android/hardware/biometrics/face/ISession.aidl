package android.hardware.biometrics.face;

import android.hardware.biometrics.common.ICancellationSignal;
import android.hardware.biometrics.common.OperationContext;
import android.hardware.biometrics.face.EnrollmentStageConfig;
import android.hardware.biometrics.face.FaceEnrollOptions;
import android.hardware.common.NativeHandle;
import android.hardware.keymaster.HardwareAuthToken;

@VintfStability
interface ISession {
    void generateChallenge();
    void revokeChallenge(in long challenge);
    EnrollmentStageConfig[] getEnrollmentConfig(in byte enrollmentType);
    ICancellationSignal enroll(in HardwareAuthToken hat, in byte type, in byte[] disabledFeatures, in NativeHandle previewSurface);
    ICancellationSignal authenticate(in long operationId);
    ICancellationSignal detectInteraction();
    void enumerateEnrollments();
    void removeEnrollments(in int[] enrollmentIds);
    void getFeatures();
    void setFeature(in HardwareAuthToken hat, in byte feature, in boolean enabled);
    void getAuthenticatorId();
    void invalidateAuthenticatorId();
    void resetLockout(in HardwareAuthToken hat);
    void close();
    ICancellationSignal authenticateWithContext(in long operationId, in OperationContext context);
    ICancellationSignal enrollWithContext(in HardwareAuthToken hat, in byte type, in byte[] disabledFeatures, in NativeHandle previewSurface, in OperationContext context);
    ICancellationSignal detectInteractionWithContext(in OperationContext context);
    void onContextChanged(in OperationContext context);
    ICancellationSignal enrollWithOptions(in FaceEnrollOptions options);

    int getInterfaceVersion();
    String getInterfaceHash();
}
