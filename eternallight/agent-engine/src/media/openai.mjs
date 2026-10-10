import { mediaTransport, mediaError, requiredText, oneOf } from "./http.mjs";
import { mediaConnectionCapabilities } from "./capabilities.mjs";

function fields(request, allowed) {
  if (!request || typeof request !== "object" || Array.isArray(request) || Object.keys(request).some(key => !allowed.includes(key))) throw mediaError("MEDIA_REQUEST_INVALID","媒体请求包含未支持的字段",{notSent:true});
}
function assetId(value) { return requiredText(value,"素材版本",200); }
function status(id,protocol,model,credentialConfigured) { return {id,capabilities:mediaConnectionCapabilities({id,protocol,model,credentialConfigured})}; }
async function imageReference(loadAsset,id) {
  const asset=await loadAsset(assetId(id));
  if (!/^image\/(png|jpeg|webp)$/.test(asset.mimeType) || !(asset.bytes instanceof Uint8Array) || !asset.bytes.length || asset.bytes.length>50*1024*1024) throw mediaError("MEDIA_INPUT_INVALID","图片素材格式或大小不受支持",{notSent:true});
  return { image_url:`data:${asset.mimeType};base64,${Buffer.from(asset.bytes).toString("base64")}` };
}
const unrecoverable = async () => ({state:"outcome_unknown"});
export function createOpenAIImagesAdapter({ id, model, loadAsset, ...transportOptions }) {
  const request=mediaTransport(transportOptions), configured=Boolean(model && transportOptions.apiKey);
  const validate = async (input,kind) => {
    oneOf(kind,["image.generate","image.edit"],"图片能力");
    if(!status(id,"openai-images",model,configured).capabilities.find(item=>item.kind===kind)?.available)throw mediaError("MEDIA_CAPABILITY_UNAVAILABLE","图片模型能力尚未配置",{notSent:true});
    fields(input,["prompt","size","quality","background","outputFormat",...(kind==="image.edit"?["imageAssetRevisionIds","maskAssetRevisionId"]:[])]);
    requiredText(input.prompt,"图片描述");
    if(input.size!==undefined) oneOf(input.size,["auto","1024x1024","1536x1024","1024x1536"],"图片尺寸");
    if(input.quality!==undefined) oneOf(input.quality,["auto","low","medium","high"],"图片质量");
    if(input.background!==undefined) oneOf(input.background,["auto","transparent","opaque"],"背景");
    if(input.outputFormat!==undefined) oneOf(input.outputFormat,["png","jpeg","webp"],"输出格式");
    if(input.background==="transparent"&&input.outputFormat==="jpeg")throw mediaError("MEDIA_REQUEST_INVALID","透明背景须使用 PNG 或 WebP",{notSent:true});
    if(kind==="image.edit" && (!Array.isArray(input.imageAssetRevisionIds) || !input.imageAssetRevisionIds.length || input.imageAssetRevisionIds.length>16)) throw mediaError("MEDIA_REQUEST_INVALID","图片编辑需要 1–16 个素材版本",{notSent:true});
    input.imageAssetRevisionIds?.forEach(assetId); if(input.maskAssetRevisionId) assetId(input.maskAssetRevisionId);
  };
  return {id, inspect:()=>status(id,"openai-images",model,configured),validate,
    async submit(input,{kind,operationId,signal}) {
      await validate(input,kind);
      const body={model,prompt:input.prompt,n:1,output_format:input.outputFormat??"png"};
      for(const key of ["size","quality","background"]) if(input[key]!==undefined) body[key]=input[key];
      if(kind==="image.edit") {body.images=await Promise.all(input.imageAssetRevisionIds.map(id=>imageReference(loadAsset,id))); if(input.maskAssetRevisionId) body.mask=await imageReference(loadAsset,input.maskAssetRevisionId);}
      const response=await request(kind==="image.edit"?"/images/edits":"/images/generations",{method:"POST",body,signal});
      if(!Array.isArray(response.data?.data) || response.data.data.length!==1) throw mediaError("MEDIA_RESPONSE_INVALID","图片服务返回的文件数量无效");
      const encoded=response.data.data[0]?.b64_json;
      if(typeof encoded!=="string" || !/^[A-Za-z0-9+/]*={0,2}$/.test(encoded)) throw mediaError("MEDIA_RESPONSE_INVALID","图片服务没有返回有效图片");
      const bytes=Buffer.from(encoded,"base64"), format=body.output_format;
      if(!bytes.length || bytes.length>50*1024*1024) throw mediaError("MEDIA_RESPONSE_INVALID","图片服务返回的文件大小无效");
      return {state:"collecting",receipt:{id:response.requestId??`openai:${operationId}`,providerId:id},usage:{units:{images:1},costMinor:null},outputs:[{name:`generated.${format}`,mimeType:`image/${format}`,bytes}]};
    },poll:unrecoverable,lookup:unrecoverable,cancel:unrecoverable,
    async fetchOutputs(result) { if(!result.outputs) throw mediaError("MEDIA_OUTPUT_UNAVAILABLE","同步图片结果无法重新获取"); return result.outputs; },
  };
}
export function createOpenAIAudioAdapter({ id, model, loadAsset, ...transportOptions }) {
  const request=mediaTransport(transportOptions),configured=Boolean(model && transportOptions.apiKey);
  const validate=async(input,kind)=>{
    oneOf(kind,["audio.speech","audio.transcribe"],"音频能力");
    if(!status(id,"openai-audio",model,configured).capabilities.find(item=>item.kind===kind)?.available)throw mediaError("MEDIA_CAPABILITY_UNAVAILABLE","音频模型不支持这项能力",{notSent:true});
    fields(input,kind==="audio.speech"?["text","voice","responseFormat","speed"]:["audioAssetRevisionId","language"]);
    if(kind==="audio.speech") {requiredText(input.text,"配音文本",4096);requiredText(input.voice,"声音",100);if(input.responseFormat!==undefined) oneOf(input.responseFormat,["mp3","opus","aac","flac","wav"],"音频格式"); if(input.speed!==undefined && (!Number.isFinite(input.speed)||input.speed<0.25||input.speed>4)) throw mediaError("MEDIA_REQUEST_INVALID","语速须在 0.25–4 之间",{notSent:true});}
    else {assetId(input.audioAssetRevisionId); if(input.language!==undefined && !/^[a-z]{2}$/.test(input.language)) throw mediaError("MEDIA_REQUEST_INVALID","语言代码无效",{notSent:true});}
  };
  return {id,inspect:()=>status(id,"openai-audio",model,configured),validate,
    async submit(input,{kind,operationId,signal}) {
      await validate(input,kind); let response,outputs;
      if(kind==="audio.speech") {
        const format=input.responseFormat??"wav";
        response=await request("/audio/speech",{method:"POST",body:{model,input:input.text,voice:input.voice,response_format:format,...(input.speed!==undefined?{speed:input.speed}:{})},signal,binary:true});
        outputs=[{name:`speech.${format}`,mimeType:({mp3:"audio/mpeg",opus:"audio/ogg",aac:"audio/aac",flac:"audio/flac",wav:"audio/wav"})[format],bytes:response.data}];
      } else {
        const asset=await loadAsset(input.audioAssetRevisionId);
        if(!/^(audio|video)\//.test(asset.mimeType) || !(asset.bytes instanceof Uint8Array) || asset.bytes.length>25*1024*1024) throw mediaError("MEDIA_INPUT_INVALID","转写素材须为不超过 25 MB 的音视频",{notSent:true});
        const form=new FormData();form.set("model",model);form.set("response_format","json");form.set("file",new Blob([asset.bytes],{type:asset.mimeType}),asset.name??"source.wav");if(input.language)form.set("language",input.language);if(model==="gpt-4o-transcribe-diarize")form.set("chunking_strategy","auto");
        response=await request("/audio/transcriptions",{method:"POST",body:form,signal});
        if(typeof response.data?.text!=="string" || response.data.text.length>1_000_000) throw mediaError("MEDIA_RESPONSE_INVALID","音频服务没有返回有效转写");
        outputs=[{kind:"transcript",text:response.data.text,sourceAssetRevisionId:input.audioAssetRevisionId}];
      }
      return {state:"collecting",receipt:{id:response.requestId??`openai:${operationId}`,providerId:id},outputs,usage:{units:kind==="audio.speech"?{characters:input.text.length}:{},costMinor:null}};
    },poll:unrecoverable,lookup:unrecoverable,cancel:unrecoverable,async fetchOutputs(result){if(!result.outputs)throw mediaError("MEDIA_OUTPUT_UNAVAILABLE","同步音频结果无法重新获取");return result.outputs;},
  };
}
