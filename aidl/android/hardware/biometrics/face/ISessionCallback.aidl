package android.hardware.biometrics.face;

import android.hardware.biometrics.face.AuthenticateSuccess;
import android.hardware.biometrics.face.AuthenticationFrame;
import android.hardware.biometrics.face.EnrollmentFrame;
import android.hardware.keymaster.HardwareAuthToken;

@VintfStability
interface ISessionCallback {
    void onChallengeGenerated(in long challenge);
    void onChallengeRevoked(in long challenge);
    void onAuthenticationFrame(in AuthenticationFrame frame);
    void onEnrollmentFrame(in EnrollmentFrame frame);
    void onError(in byte error, in int vendorCode);
    void onEnrollmentProgress(in int enrollmentId, in int remaining);
    void onAuthenticationSucceeded(in int enrollmentId, in HardwareAuthToken hat);
    void onAuthenticationFailed();
    void onLockoutTimed(in long durationMillis);
    void onLockoutPermanent();
    void onLockoutCleared();
    void onInteractionDetected();
    void onEnrollmentsEnumerated(in int[] enrollmentIds);
    void onFeaturesRetrieved(in byte[] features);
    void onFeatureSet(in byte feature);
    void onEnrollmentsRemoved(in int[] enrollmentIds);
    void onAuthenticatorIdRetrieved(in long authenticatorId);
    void onAuthenticatorIdInvalidated(in long newAuthenticatorId);
    void onSessionClosed();
    void onAuthenticationSucceededWithResult(in AuthenticateSuccess result);
}
