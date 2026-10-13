// Protocol support verified against official provider contracts, 2026-10-07.
// Credentials establish configuration, not a successful paid account probe.
const imageModels=new Set(["gpt-image-1","gpt-image-1-mini","gpt-image-1.5","chatgpt-image-latest","gpt-image-2","gpt-image-2-2026-04-21","gpt-image-2.5-sunburst","gpt-image-2.5-sunburst-2026-09-08","gpt-image-2.5-flare","gpt-image-2.5-flare-2026-09-08"]);
const speechModels=new Set(["tts-1","tts-1-hd","gpt-4o-mini-tts","gpt-4o-mini-tts-2025-12-15"]);
const transcriptionModels=new Set(["whisper-1","gpt-transcribe","gpt-4o-transcribe","gpt-4o-mini-transcribe","gpt-4o-mini-transcribe-2025-12-15","gpt-4o-transcribe-diarize"]);
export function mediaConnectionCapabilities({id,protocol,model,credentialConfigured}) {
  const support=protocol==="openai-images"?[["image.generate",imageModels.has(model)],["image.edit",imageModels.has(model)]]
    :protocol==="openai-audio"?[["audio.speech",speechModels.has(model)],["audio.transcribe",transcriptionModels.has(model)]]
    :protocol==="runway"?[["video.generate",model==="gen4.5"]]:[];
  return support.map(([kind,supported])=>({kind,providerId:id,available:Boolean(credentialConfigured&&supported),reason:!supported?"当前适配器未支持此模型的这项能力":!credentialConfigured?"请配置服务密钥":null}));
}
