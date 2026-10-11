#include <string.h>

#include <mi_ao.h>

/* Match Onion's raw OSS backend setup before sending samples to /dev/dsp. */
int slot_onion_miyoo_audio_init(int sample_rate) {
    MI_AUDIO_Attr_t attr;
    memset(&attr, 0, sizeof(attr));
    attr.eSamplerate = (MI_AUDIO_SampleRate_e)sample_rate;
    attr.eSoundmode = E_MI_AUDIO_SOUND_MODE_STEREO;
    attr.u32ChnCnt = 2;
    attr.u32PtNumPerFrm = 256;
    return MI_AO_SetPubAttr(0, &attr);
}
