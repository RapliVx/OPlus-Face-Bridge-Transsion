#include <cstdint>
#include <cstddef>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <unistd.h>
#include <sys/mman.h>
#include <errno.h>
#include <android/log.h>
#include <android/native_window.h>

#define LOG_TAG "face_transsion"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO, LOG_TAG, __VA_ARGS__)
#define LOGW(...) __android_log_print(ANDROID_LOG_WARN, LOG_TAG, __VA_ARGS__)
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, LOG_TAG, __VA_ARGS__)

#pragma pack(push, 8)
struct HidlVecUint8 {
    const uint8_t* mBuffer;
    uint32_t mSize;
    bool mOwnsBuffer;
    uint8_t mPad[3];
};
#pragma pack(pop)

typedef void* (*FnGetNativeWindow)(void* holder);
typedef void (*FnDestroySurfaceHolder)(void* holder);

struct SurfaceBridgeContext {
    void* token_utils_handle;
    void* bq1_handle;
    void* bq2_handle;
    void* bqc_handle;
    void* gui_handle;
    void* nw_handle;

    void* fn_retrieveHalInterface;
    void* fn_castFrom_v2;
    void* fn_castFrom_v1;
    void* fn_getSurfaceFromHGBP;
    FnGetNativeWindow fn_getNativeWindow;
    FnDestroySurfaceHolder fn_destroySurfaceHolder;

    // NDK NativeWindow functions
    int32_t (*fn_ANativeWindow_setBuffersGeometry)(ANativeWindow*, int32_t, int32_t, int32_t);
    int32_t (*fn_ANativeWindow_lock)(ANativeWindow*, ANativeWindow_Buffer*, ARect*);
    int32_t (*fn_ANativeWindow_unlockAndPost)(ANativeWindow*);
    void (*fn_ANativeWindow_release)(ANativeWindow*);

    void* surface_holder;
    ANativeWindow* native_window;
    int target_width;
    int target_height;
    int frame_count;
};

static void* load_lib(const char* const paths[]) {
    for (int i = 0; paths[i] != nullptr; ++i) {
        void* h = dlopen(paths[i], RTLD_NOW);
        if (h) {
            LOGI("Loaded shared library: %s (%p)", paths[i], h);
            return h;
        }
    }
    return nullptr;
}

#if defined(__aarch64__)
// retrieveHalInterface returns sp<IBase> (8 bytes) via x8
static inline void* call_retrieveHalInterface(void* fn_ptr, const void* hidl_vec_ptr) {
    struct alignas(16) {
        uint8_t data[64];
    } ret_buf;
    memset(&ret_buf, 0, sizeof(ret_buf));

    register void* x8_reg asm("x8") = &ret_buf;
    register const void* x0_reg asm("x0") = hidl_vec_ptr;
    asm volatile(
        "blr %2"
        : "+r"(x8_reg), "+r"(x0_reg)
        : "r"(fn_ptr)
        : "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x9", "x10", "x11", "x12", "x13", "x14", "x15", "x16", "x17", "x30", "memory"
    );

    return *(void**)&ret_buf.data[0];
}

// castFrom returns Return<sp<IGraphicBufferProducer>> (48 bytes) via x8
// Offset 0x28 (40) contains the sp<IGraphicBufferProducer> pointer
static inline void* call_castFrom(void* fn_ptr, const void* base_sp_ptr) {
    struct alignas(16) {
        uint8_t data[64];
    } ret_buf;
    memset(&ret_buf, 0, sizeof(ret_buf));

    register void* x8_reg asm("x8") = &ret_buf;
    register const void* x0_reg asm("x0") = base_sp_ptr;
    register uint32_t x1_reg asm("w1") = 0; // emitError = false
    asm volatile(
        "blr %3"
        : "+r"(x8_reg), "+r"(x0_reg), "+r"(x1_reg)
        : "r"(fn_ptr)
        : "x2", "x3", "x4", "x5", "x6", "x7", "x9", "x10", "x11", "x12", "x13", "x14", "x15", "x16", "x17", "x30", "memory"
    );

    return *(void**)&ret_buf.data[0x28];
}

// getSurfaceFromHGBP returns std::unique_ptr<SurfaceHolder, Deleter> (16 bytes) via x8
// Offset 0 contains the SurfaceHolder* pointer
static inline void* call_getSurfaceFromHGBP(void* fn_ptr, const void* hgbp_sp_ptr) {
    struct alignas(16) {
        uint8_t data[32];
    } ret_buf;
    memset(&ret_buf, 0, sizeof(ret_buf));

    register void* x8_reg asm("x8") = &ret_buf;
    register const void* x0_reg asm("x0") = hgbp_sp_ptr;
    asm volatile(
        "blr %2"
        : "+r"(x8_reg), "+r"(x0_reg)
        : "r"(fn_ptr)
        : "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x9", "x10", "x11", "x12", "x13", "x14", "x15", "x16", "x17", "x30", "memory"
    );

    return *(void**)&ret_buf.data[0];
}
#else
static inline void* call_retrieveHalInterface(void*, const void*) { return nullptr; }
static inline void* call_castFrom(void*, const void*) { return nullptr; }
static inline void* call_getSurfaceFromHGBP(void*, const void*) { return nullptr; }
#endif

static inline bool is_valid_native_window(ANativeWindow* anw) {
    if (!anw) return false;
    if (((uintptr_t)anw & 0x7) != 0 || (uintptr_t)anw < 0x1000) return false;
    // In ANativeWindow struct, offset 0x98 is the perform callback pointer
    void* perform_fn = *(void**)((uintptr_t)anw + 0x98);
    return perform_fn != nullptr;
}

extern "C" {

void* surface_bridge_create(const uint8_t* token_data, size_t token_len) {
    if (!token_data || token_len == 0) {
        LOGW("surface_bridge_create called with null or empty token");
        return nullptr;
    }

    LOGI("surface_bridge_create: token_len=%zu", token_len);

    auto* ctx = new SurfaceBridgeContext();
    memset(ctx, 0, sizeof(*ctx));
    ctx->target_width = 480;
    ctx->target_height = 640;

    const char* token_paths[] = {
        "/apex/com.android.vndk.v31/lib64/android.hidl.token@1.0-utils.so",
        "/apex/com.android.vndk.v31@1/lib64/android.hidl.token@1.0-utils.so",
        "/system/lib64/android.hidl.token@1.0-utils.so",
        "android.hidl.token@1.0-utils.so",
        nullptr
    };
    ctx->token_utils_handle = load_lib(token_paths);
    if (!ctx->token_utils_handle) {
        LOGE("Failed to dlopen android.hidl.token@1.0-utils.so");
        delete ctx;
        return nullptr;
    }

    ctx->fn_retrieveHalInterface = dlsym(ctx->token_utils_handle, "_ZN7android20retrieveHalInterfaceERKNS_8hardware8hidl_vecIhEE");
    if (!ctx->fn_retrieveHalInterface) {
        LOGE("Failed to find retrieveHalInterface in token utils");
        delete ctx;
        return nullptr;
    }

    const char* bq2_paths[] = {
        "/apex/com.android.vndk.v31/lib64/android.hardware.graphics.bufferqueue@2.0.so",
        "/apex/com.android.vndk.v31@1/lib64/android.hardware.graphics.bufferqueue@2.0.so",
        "/system/lib64/android.hardware.graphics.bufferqueue@2.0.so",
        "android.hardware.graphics.bufferqueue@2.0.so",
        nullptr
    };
    ctx->bq2_handle = load_lib(bq2_paths);
    if (ctx->bq2_handle) {
        ctx->fn_castFrom_v2 = dlsym(ctx->bq2_handle, "_ZN7android8hardware8graphics11bufferqueue4V2_022IGraphicBufferProducer8castFromERKNS_2spINS_4hidl4base4V1_05IBaseEEEb");
    }

    const char* bq1_paths[] = {
        "/apex/com.android.vndk.v31/lib64/android.hardware.graphics.bufferqueue@1.0.so",
        "/apex/com.android.vndk.v31@1/lib64/android.hardware.graphics.bufferqueue@1.0.so",
        "/system/lib64/android.hardware.graphics.bufferqueue@1.0.so",
        "android.hardware.graphics.bufferqueue@1.0.so",
        nullptr
    };
    ctx->bq1_handle = load_lib(bq1_paths);
    if (ctx->bq1_handle) {
        ctx->fn_castFrom_v1 = dlsym(ctx->bq1_handle, "_ZN7android8hardware8graphics11bufferqueue4V1_022IGraphicBufferProducer8castFromERKNS_2spINS_4hidl4base4V1_05IBaseEEEb");
    }

    const char* bqc_paths[] = {
        "/apex/com.android.vndk.v31/lib64/libbufferqueueconverter.so",
        "/apex/com.android.vndk.v31@1/lib64/libbufferqueueconverter.so",
        "/system/lib64/libbufferqueueconverter.so",
        "libbufferqueueconverter.so",
        nullptr
    };
    ctx->bqc_handle = load_lib(bqc_paths);
    if (!ctx->bqc_handle) {
        LOGE("Failed to dlopen libbufferqueueconverter.so");
        delete ctx;
        return nullptr;
    }

    ctx->fn_getSurfaceFromHGBP = dlsym(ctx->bqc_handle, "_ZN7android18getSurfaceFromHGBPERKNS_2spINS_8hardware8graphics11bufferqueue4V2_022IGraphicBufferProducerEEE");
    ctx->fn_getNativeWindow = (FnGetNativeWindow)dlsym(ctx->bqc_handle, "_ZN7android15getNativeWindowEPNS_13SurfaceHolderE");
    ctx->fn_destroySurfaceHolder = (FnDestroySurfaceHolder)dlsym(ctx->bqc_handle, "_ZN7android20destroySurfaceHolderEPNS_13SurfaceHolderE");

    if (!ctx->fn_getSurfaceFromHGBP || !ctx->fn_getNativeWindow || !ctx->fn_destroySurfaceHolder) {
        LOGE("Failed to find bufferqueueconverter symbols (getSurface=%p, getWindow=%p, destroy=%p)",
             ctx->fn_getSurfaceFromHGBP, ctx->fn_getNativeWindow, ctx->fn_destroySurfaceHolder);
        delete ctx;
        return nullptr;
    }

    const char* nw_paths[] = {
        "/system/lib64/libnativewindow.so",
        "libnativewindow.so",
        nullptr
    };
    ctx->nw_handle = load_lib(nw_paths);
    if (ctx->nw_handle) {
        ctx->fn_ANativeWindow_setBuffersGeometry = (decltype(ctx->fn_ANativeWindow_setBuffersGeometry))dlsym(ctx->nw_handle, "ANativeWindow_setBuffersGeometry");
        ctx->fn_ANativeWindow_lock = (decltype(ctx->fn_ANativeWindow_lock))dlsym(ctx->nw_handle, "ANativeWindow_lock");
        ctx->fn_ANativeWindow_unlockAndPost = (decltype(ctx->fn_ANativeWindow_unlockAndPost))dlsym(ctx->nw_handle, "ANativeWindow_unlockAndPost");
        ctx->fn_ANativeWindow_release = (decltype(ctx->fn_ANativeWindow_release))dlsym(ctx->nw_handle, "ANativeWindow_release");
    }

    HidlVecUint8 vec;
    vec.mBuffer = token_data;
    vec.mSize = (uint32_t)token_len;
    vec.mOwnsBuffer = false;
    vec.mPad[0] = 0; vec.mPad[1] = 0; vec.mPad[2] = 0;

    void* base_sp = call_retrieveHalInterface(ctx->fn_retrieveHalInterface, &vec);
    LOGI("retrieveHalInterface returned base_sp = %p", base_sp);
    if (!base_sp) {
        LOGE("Failed to retrieve HAL interface from token!");
        delete ctx;
        return nullptr;
    }

    void* hgbp_sp = nullptr;
    bool is_v1 = false;
    if (ctx->fn_castFrom_v2) {
        hgbp_sp = call_castFrom(ctx->fn_castFrom_v2, &base_sp);
        LOGI("castFrom V2_0 returned hgbp_sp = %p", hgbp_sp);
    }
    if (!hgbp_sp && ctx->fn_castFrom_v1) {
        LOGI("Token is not V2_0; attempting castFrom V1_0...");
        hgbp_sp = call_castFrom(ctx->fn_castFrom_v1, &base_sp);
        LOGI("castFrom V1_0 returned hgbp_sp = %p", hgbp_sp);
        if (hgbp_sp) {
            is_v1 = true;
        }
    }
    if (!hgbp_sp) {
        LOGW("castFrom returned null for both V2_0 and V1_0. Disabling preview surface bridge safely.");
        delete ctx;
        return nullptr;
    }

    // For V1_0 tokens, patch libbufferqueueconverter GOT to use V1_0 H2BGraphicBufferProducer vtable
    if (is_v1) {
        const char* gui_paths[] = {
            "/apex/com.android.vndk.v31/lib64/libgui.so",
            "/apex/com.android.vndk.v31@1/lib64/libgui.so",
            "/system/lib64/libgui.so",
            "libgui.so",
            nullptr
        };
        ctx->gui_handle = load_lib(gui_paths);

        void* ztt_v1 = nullptr;
        void* ztv_v1 = nullptr;
        if (ctx->gui_handle) {
            ztt_v1 = dlsym(ctx->gui_handle, "_ZTTN7android8hardware8graphics11bufferqueue4V1_05utils24H2BGraphicBufferProducerE");
            ztv_v1 = dlsym(ctx->gui_handle, "_ZTVN7android8hardware8graphics11bufferqueue4V1_05utils24H2BGraphicBufferProducerE");
        }
        if (!ztt_v1 || !ztv_v1) {
            ztt_v1 = dlsym(ctx->bqc_handle, "_ZTTN7android8hardware8graphics11bufferqueue4V1_05utils24H2BGraphicBufferProducerE");
            ztv_v1 = dlsym(ctx->bqc_handle, "_ZTVN7android8hardware8graphics11bufferqueue4V1_05utils24H2BGraphicBufferProducerE");
        }
        if (!ztt_v1 || !ztv_v1) {
            ztt_v1 = dlsym(RTLD_DEFAULT, "_ZTTN7android8hardware8graphics11bufferqueue4V1_05utils24H2BGraphicBufferProducerE");
            ztv_v1 = dlsym(RTLD_DEFAULT, "_ZTVN7android8hardware8graphics11bufferqueue4V1_05utils24H2BGraphicBufferProducerE");
        }

        if (!ztt_v1 || !ztv_v1) {
            LOGE("Failed to resolve V1_0 H2B symbols in libgui.so / bqc (ztt=%p, ztv=%p)", ztt_v1, ztv_v1);
            delete ctx;
            return nullptr;
        }

        Dl_info info;
        uintptr_t bqc_base = 0;
        if (dladdr((void*)ctx->fn_destroySurfaceHolder, &info) && info.dli_fbase) {
            bqc_base = (uintptr_t)info.dli_fbase;
        } else {
            bqc_base = (uintptr_t)ctx->fn_destroySurfaceHolder - 0x1014;
        }
        LOGI("libbufferqueueconverter base address: %p", (void*)bqc_base);

        long page_size = sysconf(_SC_PAGESIZE);
        if (page_size <= 0) page_size = 4096;
        uintptr_t got_addr = bqc_base + 0x2240;
        uintptr_t page_start = got_addr & ~(page_size - 1);
        if (mprotect((void*)page_start, page_size, PROT_READ | PROT_WRITE) != 0) {
            LOGE("mprotect failed for GOT page %p: %s", (void*)page_start, strerror(errno));
            delete ctx;
            return nullptr;
        }

        LOGI("Patching libbufferqueueconverter GOT: old ZTT=%p, old ZTV=%p",
             *(void**)(bqc_base + 0x2240), *(void**)(bqc_base + 0x2248));
        *(void**)(bqc_base + 0x2240) = ztt_v1;
        *(void**)(bqc_base + 0x2248) = ztv_v1;
        mprotect((void*)page_start, page_size, PROT_READ);
        LOGI("Successfully patched libbufferqueueconverter GOT for V1_0: new ZTT=%p, new ZTV=%p",
             *(void**)(bqc_base + 0x2240), *(void**)(bqc_base + 0x2248));
    }

    void* holder = call_getSurfaceFromHGBP(ctx->fn_getSurfaceFromHGBP, &hgbp_sp);
    LOGI("getSurfaceFromHGBP returned holder = %p", holder);
    if (!holder) {
        LOGE("Failed to get SurfaceHolder from IGraphicBufferProducer!");
        delete ctx;
        return nullptr;
    }
    ctx->surface_holder = holder;

    ANativeWindow* anw = (ANativeWindow*)ctx->fn_getNativeWindow(holder);
    LOGI("getNativeWindow returned anw = %p", anw);
    if (!anw || !is_valid_native_window(anw)) {
        LOGE("Failed to get valid ANativeWindow from SurfaceHolder (anw=%p)!", anw);
        ctx->fn_destroySurfaceHolder(ctx->surface_holder);
        ctx->surface_holder = nullptr;
        delete ctx;
        return nullptr;
    }
    ctx->native_window = anw;

    if (ctx->fn_ANativeWindow_setBuffersGeometry) {
        int ret = ctx->fn_ANativeWindow_setBuffersGeometry(anw, ctx->target_width, ctx->target_height, WINDOW_FORMAT_RGBA_8888);
        LOGI("ANativeWindow_setBuffersGeometry(%d, %d, RGBA_8888) returned %d", ctx->target_width, ctx->target_height, ret);
        if (ret != 0) {
            LOGW("ANativeWindow_setBuffersGeometry failed with RGBA_8888; attempting with default geometry...");
            ret = ctx->fn_ANativeWindow_setBuffersGeometry(anw, 0, 0, 0);
            LOGI("Fallback ANativeWindow_setBuffersGeometry(0, 0, 0) returned %d", ret);
        }
    }

    LOGI("SurfaceBridge successfully created and initialized (mode: %s).",
         is_v1 ? "V1_0" : "V2_0");
    return ctx;
}

int surface_bridge_render_frame(void* bridge_ctx, const uint8_t* nv21_data, int width, int height) {
    if (!bridge_ctx || !nv21_data || width <= 0 || height <= 0) {
        return -1;
    }

    auto* ctx = (SurfaceBridgeContext*)bridge_ctx;
    if (!ctx->native_window || !ctx->fn_ANativeWindow_lock || !ctx->fn_ANativeWindow_unlockAndPost) {
        return -2;
    }

    if (!is_valid_native_window(ctx->native_window)) {
        if (ctx->frame_count == 0) {
            LOGE("Cannot render frame: native_window perform callback is null");
        }
        return -4;
    }

    ANativeWindow_Buffer buffer;
    memset(&buffer, 0, sizeof(buffer));

    int lock_ret = ctx->fn_ANativeWindow_lock(ctx->native_window, &buffer, nullptr);
    if (lock_ret != 0 || !buffer.bits) {
        if (ctx->frame_count == 0 || ctx->frame_count % 30 == 0) {
            LOGW("ANativeWindow_lock returned %d (bits=%p, w=%d, h=%d, stride=%d, format=%d)",
                 lock_ret, buffer.bits, buffer.width, buffer.height, buffer.stride, buffer.format);
        }
        return -3;
    }

    const int buf_w = buffer.width;
    const int buf_h = buffer.height;
    const int buf_stride = buffer.stride;
    if (buf_w <= 0 || buf_h <= 0 || buf_stride <= 0) {
        ctx->fn_ANativeWindow_unlockAndPost(ctx->native_window);
        return -5;
    }

    const uint8_t* y_plane = nv21_data;
    const uint8_t* vu_plane = nv21_data + (width * height);

    // Precompute sensor coordinates to remove integer division from pixel loops
    int* col_lut = (int*)alloca(buf_h * sizeof(int));
    int* row_lut = (int*)alloca(buf_w * sizeof(int));
    int* row_half_w_lut = (int*)alloca(buf_w * sizeof(int));

    const int h_denom = (buf_h > 1) ? (buf_h - 1) : 1;
    for (int dy = 0; dy < buf_h; ++dy) {
        int col = ((width - 1) * (buf_h - 1 - dy)) / h_denom;
        if (col < 0) col = 0;
        else if (col >= width) col = width - 1;
        col_lut[dy] = col;
    }

    const int w_denom = (buf_w > 1) ? (buf_w - 1) : 1;
    for (int dx = 0; dx < buf_w; ++dx) {
        int row = ((height - 1) * dx) / w_denom;
        if (row < 0) row = 0;
        else if (row >= height) row = height - 1;
        row_lut[dx] = row;
        row_half_w_lut[dx] = (row >> 1) * width;
    }

    if (buffer.format == 4) { // WINDOW_FORMAT_RGB_565
        uint16_t* dst_pixels16 = (uint16_t*)buffer.bits;
        for (int dy = 0; dy < buf_h; ++dy) {
            const int sensor_col = col_lut[dy];
            const int col_uv_base = (sensor_col >> 1) << 1;
            const uint8_t* y_col_ptr = y_plane + sensor_col;
            uint16_t* row_dst = dst_pixels16 + dy * buf_stride;

            for (int dx = 0; dx < buf_w; ++dx) {
                const int sensor_row = row_lut[dx];
                const uint8_t y_val = y_col_ptr[sensor_row * width];
                const int uv_idx = row_half_w_lut[dx] + col_uv_base;
                const uint8_t v_val = vu_plane[uv_idx];
                const uint8_t u_val = vu_plane[uv_idx + 1];

                int c = (int)y_val - 16;
                if (c < 0) c = 0;
                int d = (int)u_val - 128;
                int e = (int)v_val - 128;

                int r = (298 * c + 409 * e + 128) >> 8;
                int g = (298 * c - 100 * d - 208 * e + 128) >> 8;
                int b = (298 * c + 516 * d + 128) >> 8;

                if (r < 0) r = 0; else if (r > 255) r = 255;
                if (g < 0) g = 0; else if (g > 255) g = 255;
                if (b < 0) b = 0; else if (b > 255) b = 255;

                row_dst[dx] = (uint16_t)(((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3));
            }
        }
    } else { // 32-bit (RGBA_8888, RGBX_8888)
        uint32_t* dst_pixels = (uint32_t*)buffer.bits;
        for (int dy = 0; dy < buf_h; ++dy) {
            const int sensor_col = col_lut[dy];
            const int col_uv_base = (sensor_col >> 1) << 1;
            const uint8_t* y_col_ptr = y_plane + sensor_col;
            uint32_t* row_dst = dst_pixels + dy * buf_stride;

            for (int dx = 0; dx < buf_w; ++dx) {
                const int sensor_row = row_lut[dx];
                const uint8_t y_val = y_col_ptr[sensor_row * width];
                const int uv_idx = row_half_w_lut[dx] + col_uv_base;
                const uint8_t v_val = vu_plane[uv_idx];
                const uint8_t u_val = vu_plane[uv_idx + 1];

                int c = (int)y_val - 16;
                if (c < 0) c = 0;
                int d = (int)u_val - 128;
                int e = (int)v_val - 128;

                int r = (298 * c + 409 * e + 128) >> 8;
                int g = (298 * c - 100 * d - 208 * e + 128) >> 8;
                int b = (298 * c + 516 * d + 128) >> 8;

                if (r < 0) r = 0; else if (r > 255) r = 255;
                if (g < 0) g = 0; else if (g > 255) g = 255;
                if (b < 0) b = 0; else if (b > 255) b = 255;

                row_dst[dx] = (uint32_t)((0xFF << 24) | ((uint8_t)b << 16) | ((uint8_t)g << 8) | (uint8_t)r);
            }
        }
    }

    ctx->fn_ANativeWindow_unlockAndPost(ctx->native_window);
    ctx->frame_count++;

    if (ctx->frame_count == 1 || ctx->frame_count % 60 == 0) {
        LOGI("Rendered preview frame %d onto Surface (%dx%d, stride=%d, format=%d)",
             ctx->frame_count, buf_w, buf_h, buf_stride, buffer.format);
    }
    return 0;
}

void surface_bridge_destroy(void* bridge_ctx) {
    if (!bridge_ctx) return;
    auto* ctx = (SurfaceBridgeContext*)bridge_ctx;
    LOGI("surface_bridge_destroy: total frames rendered = %d", ctx->frame_count);

    // Note: Do not call ANativeWindow_release here. Ownership of the underlying
    // Surface is held by SurfaceHolder; calling destroySurfaceHolder decrements the strong ref.
    ctx->native_window = nullptr;

    if (ctx->surface_holder && ctx->fn_destroySurfaceHolder) {
        ctx->fn_destroySurfaceHolder(ctx->surface_holder);
        ctx->surface_holder = nullptr;
    }

    delete ctx;
}

int sensetime_set_orientation(int orientation) {
    typedef int (*FnSetOrientation)(int);
    static FnSetOrientation fn = nullptr;
    static bool resolved = false;

    if (!resolved) {
        resolved = true;
        void* sym = dlsym(RTLD_DEFAULT, "st_auth_set_face_orientation");
        if (!sym) {
            void* h = dlopen("/vendor/lib64/libstfaceunlockppl.so", RTLD_NOW);
            if (h) {
                sym = dlsym(h, "st_auth_set_face_orientation");
            }
        }
        fn = (FnSetOrientation)sym;
    }

    if (fn) {
        int ret = fn(orientation);
        LOGI("sensetime_set_orientation(%d) returned %d", orientation, ret);
        return ret;
    } else {
        LOGE("st_auth_set_face_orientation symbol NOT found!");
        return -1;
    }
}

} // extern "C"
