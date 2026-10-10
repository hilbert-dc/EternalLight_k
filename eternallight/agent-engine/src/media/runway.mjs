import { mediaTransport, downloadMedia, mediaError, oneOf, requiredText } from "./http.mjs";
import { mediaConnectionCapabilities } from "./capabilities.mjs";

/** Runway gen4.5's current official task API. No unverified idempotency retry. */
export function createRunwayAdapter({id,model,loadAsset,outputHosts=["cloudfront.net","runwayml.com","runwaycdn.com"],...transportOptions}) {
  const request=mediaTransport({...transportOptions,headers:{"X-Runway-Version":"2024-11-06"}});
  const configured=Boolean(transportOptions.apiKey)&&model==="gen4.5";
  const validate=async(input,kind)=>{
    oneOf(kind,["video.generate"],"视频能力");
    if(!input || Object.keys(input).some(key=>!["prompt","imageAssetRevisionId","duration","ratio"].includes(key)))throw mediaError("MEDIA_REQUEST_INVALID","视频请求包含未支持字段",{notSent:true});
    requiredText(input.prompt,"视频描述",1000);if(!Number.isInteger(input.duration)||input.duration<2||input.duration>10)throw mediaError("MEDIA_REQUEST_INVALID","视频时长须为 2–10 秒",{notSent:true});
    oneOf(input.ratio,input.imageAssetRevisionId?["1280:720","720:1280","1104:832","960:960","832:1104","1584:672"]:["1280:720","720:1280"],"视频比例");
    if(input.imageAssetRevisionId)requiredText(input.imageAssetRevisionId,"图片版本",200);
    if(!configured)throw mediaError("MEDIA_CAPABILITY_UNAVAILABLE","请配置 Runway gen4.5 服务",{notSent:true});
  };
  const poll=async(jobId,{signal}={})=>{
    requiredText(jobId,"视频作业编号",200);
    const response=await request(`/v1/tasks/${encodeURIComponent(jobId)}`,{signal}),task=response.data;
    const state=({PENDING:"submitted",THROTTLED:"submitted",RUNNING:"running",SUCCEEDED:"collecting",FAILED:"failed",CANCELLED:"cancelled"})[task?.status];
    if(!state || task.id && task.id!==jobId)throw mediaError("MEDIA_RESPONSE_INVALID","视频作业状态无效");
    const credits=task.cost?.credits;
    const receipt={id:`runway:${jobId}:${task.status}`,providerId:id,status:task.status,...(Number.isFinite(credits)?{credits}:{})};
    return {state,jobId,receipt,outputUrls:state==="collecting"?task.output:undefined,pollAfterMs:5000,usage:{units:{},costMinor:null}};
  };
  return {id,inspect:()=>({id,capabilities:mediaConnectionCapabilities({id,protocol:"runway",model,credentialConfigured:Boolean(transportOptions.apiKey)})}),validate,
    async submit(input,{kind,signal}){
      await validate(input,kind);const body={model,promptText:input.prompt,duration:input.duration,ratio:input.ratio};
      if(input.imageAssetRevisionId){const asset=await loadAsset(input.imageAssetRevisionId);if(!/^image\/(png|jpeg|webp)$/.test(asset.mimeType)||!(asset.bytes instanceof Uint8Array)||asset.bytes.length>5*1024*1024)throw mediaError("MEDIA_INPUT_INVALID","视频参考图须为不超过 5 MB 的图片",{notSent:true});body.promptImage=`data:${asset.mimeType};base64,${Buffer.from(asset.bytes).toString("base64")}`;}
      const response=await request(input.imageAssetRevisionId?"/v1/image_to_video":"/v1/text_to_video",{method:"POST",body,signal});
      requiredText(response.data?.id,"视频作业编号",200);
      return {state:"submitted",jobId:response.data.id,receipt:{id:`runway:${response.data.id}:submitted`,providerId:id},pollAfterMs:5000};
    },poll,async lookup({jobId},context){return jobId?poll(jobId,context):{state:"outcome_unknown"};},
    async cancel(jobId,context){
      const status=await poll(jobId,context);if(["collecting","failed","cancelled"].includes(status.state))return status;
      await request(`/v1/tasks/${encodeURIComponent(jobId)}`,{method:"DELETE",signal:context?.signal});
      // Acknowledged cancellation is not a refund receipt; budget stays reserved.
      return {state:"cancelled",jobId,receipt:{id:`runway:${jobId}:cancelled`,providerId:id,status:"CANCELLED"},usage:{units:{},costMinor:null}};
    },async fetchOutputs(result,{signal}={}){
      // gen4.5 requests reserve one video's duration. An unexpected batch must
      // remain uncertain instead of silently importing beyond that grant.
      if(!Array.isArray(result.outputUrls)||result.outputUrls.length!==1)throw mediaError("MEDIA_RESPONSE_INVALID","视频作业未返回单个有效文件，请核对服务结果");
      const outputs=[];for(let index=0;index<result.outputUrls.length;index++){const output=await downloadMedia(result.outputUrls[index],{fetchImpl:transportOptions.fetchImpl,signal,allowedHosts:outputHosts});outputs.push({...output,name:`video-${index+1}.mp4`});}return outputs;
    },
  };
}
