import type { OutlineNode } from "./types";

export const outlineLayerCopy: Record<OutlineNode["kind"], {
  purpose: string; summary: string; purposeHint: string; summaryHint: string; discussion: string;
}> = {
  book: {
    purpose: "核心主线", summary: "故事脉络",
    purposeHint: "主角想达成什么，主要冲突是什么，失败会失去什么",
    summaryHint: "故事因何开始，如何推进与转折，高潮中做出什么选择，最终如何收束；未决定的部分可注明待定",
    discussion: "聚焦整部故事的因果脉络：起因、主动推进、关键转折、高潮选择与后果、收束。不要逐卷复制情节；尚未决定的部分先讨论，不强行补定结局。",
  },
  volume: {
    purpose: "本卷推动什么", summary: "本卷事件与变化",
    purposeHint: "这一卷让主线走到哪一步，人物或局势发生什么关键变化",
    summaryHint: "本卷从什么局面开始，哪些事件推动变化，结束时留下什么新局面；具体章场景在近章规划中展开",
    discussion: "聚焦这一卷在全书主线中的作用、阶段事件的因果及起止局面，不重复整部故事的脉络，不扩写其他卷。",
  },
  unit: {
    purpose: "故事目标", summary: "故事安排",
    purposeHint: "这一段在故事里做什么",
    summaryHint: "这里发生什么，故事怎样向前发展",
    discussion: "聚焦这一段如何推动所属阶段，保持与上级主线和卷纲一致。",
  },
};
