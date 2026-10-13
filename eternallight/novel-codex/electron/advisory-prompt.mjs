// @ts-check
import { composeNovelCodexPrompt } from "./codex-runtime.mjs";
import { QUALITY_NOTEBOOK_INSTRUCTIONS } from "./quality-notebook.mjs";
import { QUALITY_DELIVERY_REVIEW_INSTRUCTIONS } from "./creative-mission.mjs";
import { conversationFocusInstructions, CONVERSATION_ANALYSIS_INSTRUCTIONS } from "./conversation-analysis.mjs";
import { CONTEXT_READING_INSTRUCTIONS, TOOL_PROGRESS_INSTRUCTIONS, REQUIREMENT_SCOPE_INSTRUCTIONS } from "./context-reading-contract.mjs";
import { REVISION_REPAIR_INSTRUCTIONS } from "./revision-repair-handlers.mjs";
import { SCOPED_REVISION_INSTRUCTIONS, taskMethodText } from "./workflow-handoff.mjs";

export const POST_EDIT_INSPECTION_INSTRUCTIONS = '按read_work_status的receipts.items中generateChapter回执识别实际新稿；canFinish=true不是故事通过。仅措辞、错字或标点润色核对局部；人物状态、时间、金额、持物、因果或伏笔变化读相关前文。required=false的modelKnowledge等工具是可选能力，作者明确要求的效果评估仍须完成。先read_context当前目录，sourceId/cursor不跨任务；完整读回沿分页实际读完，检索零命中不能代替全文。sourceChecks记录当前来源版本、哈希及精确原句，检查coverage和pendingSources；缺失或未覆盖内容明确说明，不宣称全书通过。发现授权内问题继续定向修订；不得因此自动改写其他章节、发布或确认设定。';


export const AGENT_STRATEGY_INSTRUCTIONS = '按作者目标选择步骤：先核对作者要求、已有方向及当前工作所需准备；新长篇先形成适用的全局设计与近期安排，明确委托则自主选择，关键未知未委托则协商。已有规划充分可writeNextChapter；有界试写或局部任务不强制全书规划。需要简报/大纲/规划/检查时先取得真实回执再续写；遵守绑定规划，不固定规划章数，不每章强制全部知识/伏笔工具。按回执chapterNumber/title识别章节。起草附chapterAcceptance={requirements:[{id,text}],length:{requirement,minCharacters,maxCharacters}}，标准id跨修订稳定；无篇幅限制用null，修订复用当前chapterAcceptance。按冻结verification选择readyActions中verifyChapter或assessRevisionOutcome的真实actionKey，Native派生writer和draft绑定。篇幅、连续性或章纲未满足时在当前章授权内补改，不延期后finishChapter；finishChapter不传chapterReview，由Native复用当前有效报告、冻结标准和真实篇幅；verifyChapter仍提交原文支持的逐项判断。readyActions含recovery先读失败及保存结果；planRequired=true时附recoveryPlan={failedTaskId,cause,change,verification}，不扩目标或预算。未知写先核对。generationSceneFeedback收尾前读取；sourceCurrent=false补读当前来源，未解决不能称通过。';


// Policy establishes responsibility without choosing any fixed story operation.
export const AUTONOMOUS_CREATION_INSTRUCTIONS = '一次作者输入后在原Work自主交付。新章和解决阻断问题的修订须核验；按作者要求冻结chapterAcceptance.verification为direct或independent，明确要求独立时必须independent，不覆盖已冻结方式；确有问题才定向修订复评，不强制无问题任务重写。起草前chapterAcceptance.requirements列稳定{id,text}及实际篇幅边界。解释有误可用interpretationCorrection={reason,impact,sourceChecks:[{sourceId,contentHash,quote}]}引已读authorRequirementsSource原话；不改标准让现有未达篇幅稿通过。generateChapter真实保存后按冻结方式选择verifyChapter直接核验或assessRevisionOutcome独立核验。两种核验均完整读原话、当前稿和必要原稿/前文，不继承writer成功宣称，检查清单遗漏并按schema列uncoveredRequirements及原话证据。标准id不按序号重建；负评和证据不足通过真实报告登记issue，选择已有issueId或新问题，不按措辞自动合并。相同问题跨修订沿用真实issueId；按需read_work_issues取得实际observationId，复评通过时以resolutionOf显式引用被重新核对的观点。另建问题不关闭旧问题，新稿也不自动解决旧问题；冲突或过期不能称通过。决策读requirements[kind=revisionOutcome].assessment和read_revision_outcome(writerTaskId)当前source，沿read_source_content读完整报告及引文，textDelivered=false目录不算已读。verdict=passed、current=true、length.satisfied=true及适用问题已核对才可交付；pending/stale/notMet/insufficientEvidence不推进。根据真实问题选择原授权内修改、补读或有依据整章重写，保存后按冻结方式重新核验；不等作者逐条纠错。重复问题应改变方法，无新证据不重复同参查询或全文复制。来源进度仅是已返回原文，不是理解证明；压缩后按需读回计划、来源、回执和问题。预算或权限不足则明确blocked及原因，未知写先核对，不另建根任务扩大预算。';

// Goal status describes substantive delivery, separately from following the
// author's instructions about how to explain unavailable work.
export const DELIVERY_GOAL_STATUS_INSTRUCTIONS = '任何作者要求的创作/检查未完成，goalStatus=partial；unresolved按当前schema引用真实issue及observation，非正文缺口可在同一审查登记新问题。issueResolutions仅复核已有非正文问题并引用真实resolutionOf，不能自评消除正文独立负评；无问题用[]。checkedScope列实际覆盖，不以解释代替完成。缺失前文不能称衔接通过。真正需要作者决定情节或范围才needsAuthorDecision并列authorDecisions，来源不足不制造选择。所有实质目标完成才fulfilled；明确排除的检查不计入目标。最终回复与自检一致，已保存成果不因检查缺口失效。';

// Production and live acceptance share the same bounded author/source instructions.
export function composeAdvisoryPrompt(task, prompt, handoff, focus = null) {
  const target = task.target;
  const decision = task.inputManifest?.workDecision;
  const ordinary = !decision && !target.revisionRepairSource;
  const actionInstructions = decision
    ? '作者明确要求改编时，可用read_adaptation_sources读取当前任务固定来源，再用request_media_adaptation登记漫画/动画后继步骤；保存来源后才能继续依赖改编，登记后完成本步骤让Host派发，原Work最终交付核对仍需完成。本轮是同Work决策，先read_work_status核requirements、readyActions、receipts及完整目标，required不穷尽作者要求。request_work_action选真实actionKey，或proposal={operation,target}提出原授权内必要任务；正文用真实chapterId和scope={type:"selectedScenes",groupIds:[真实场景ID]}，资料用本Work sourceTaskId。Native派生绑定/CAS，不request_candidate_task。verifyChapter是在当前决策内保存直接核验报告，先完整读本任务authorRequirementsSource及当前正文，前一任务读过不等于本任务已读，提交带原文依据的chapterReview后再读状态继续决策；其他执行任务登记后complete_task交Host执行，保存后再进入决策；不等待或重复登记。新范围request_generation_handoff须作者批准，原范围用resumeRun。按需分页补齐必要信息；receipts.coverage.complete=true表示回执已全，不因其他目录分页重复查回执。deliveryReviewRequired=true时complete_task交deliveryReview，expectedWorkVersion/expectedSourceDigest复制最新状态；goalStatus=fulfilled/partial/needsAuthorDecision，goalAssessment与checkedScope说明实际结果及范围，unresolved引用未解决问题及真实观点、authorDecisions列真待决定，evidence默认省略，由Native绑定本Work全部当前有效成果；显式提供时仍须准确完整。finalDeliveryReviewRequired=true时最后finishChapter后读新状态提交总自检。worker不提交自检；fulfilled必须全部通过无遗留，partial须真实未完成，不能以此规避前置。回复只讲真实结果、范围与不足；精确数量只据receipts.items[].outputs.characterCount，称正文字符数（不含空白、含标点），不估算或展示技术字段。'

    : target.revisionRepairSource ? REVISION_REPAIR_INSTRUCTIONS
      : '作者明确要求修改现有作品时，按精确目标request_candidate_task；登记后complete_task交还Host，不在同轮等待保存或重复登记。Host执行后进入本Work核对决策。明确要求新章/连续创作时，先区分恢复原范围或提出新范围，再使用request_generation_handoff；不猜章节ID或创建占位。纯讨论不登记写作。expectedIntentRevision使用当前作者意图代次，初始0。';
  const recoveryInstructions = ordinary && task.inputManifest?.workDecisionAction?.operation === "advisory"
    ? "本轮恢复的是原普通问答，不是上一轮Work决策，也不是正文writer。以本轮read_context和工具目录为准，历史决策的sourceId、游标和仅用request_work_action的阶段限制不延续到本轮。读取当前原要求及目标后，明确修改用request_candidate_task登记精确候选，再complete_task交还Host执行；不能将恢复问答本身当作已执行修改。" : "";
  const planInstructions = '原要求/权限为准，解释和计划可纠错。简单任务直接行动；多步可read_work_plan/update_work_plan保存0—12项近期窗口，目标/预算沿用原Work。方法目标与依赖可撤销、合并、替换、重排；approach解释重要调整，planVersion查历史。计划不穷尽作者要求；删目标或改planningOnly不解除既有成果/独立检查义务，也不证明完成。读authorRequirementsSource核对遗漏。压缩后按需回读计划、原要求、回执、问题，不重发原话或新建根任务。换源重查，无新证据不重复调用，不扩权限或累计预算。';
  const creationRoute = !target.revisionRepairSource && !target.runId ? "根据作者原文与对话理解目标，不按‘继续’等关键词路由。涉及连续任务先read_work_status(view=conversationRuns)：恢复已有范围时选择对应原任务，request_generation_handoff传resumeRun={id,expectedVersion}及原targetChapters，沿用预算；须先修改/检查/讨论则先完成前置，再登记resumeRun交接，将前置通过与真实交接评为fulfilled交Host继续，这不表示长篇已完成。partial或needsAuthorDecision不续接。真实新范围才不带resumeRun提议。作者要求暂停原任务时，由模型选择真实runId并调用pause_generation_run，expectedVersion及expectedIntentRevision复制当前状态；真实committedGenerationControl回执才说明已暂停，不以自然语言承诺代替。否定、假设、询问不授权执行；实际歧义才问。conversationRuns不包含其他讨论成果，不能从摘要缺失猜未保存；说明当前阻碍需读真实状态，其他成果未核对就如实说明。" : "";
  return composeNovelCodexPrompt(taskMethodText(task), task.inputManifest.authorIntent, `${planInstructions}\n\n${decision ? `${REQUIREMENT_SCOPE_INSTRUCTIONS}\n\n${AUTONOMOUS_CREATION_INSTRUCTIONS}` : "在原Work及预算内完成作者目标；章节起草前冻结验收标准，保存后按作者要求冻结的直接或独立方式核验当前稿，读失败报告定向修订并复评，通过后交付或接续，不等外部逐条纠错。"}\n\n${creationRoute}\n\n${recoveryInstructions}\n\n${actionInstructions}\n\n${decision ? QUALITY_NOTEBOOK_INSTRUCTIONS : "按需read_quality_notebook/record_quality_note，证据锚定真实当前稿；偏好仅软指导，不扩大权限或制造文学总分。独立阅读可准备包并由后续Work决策选择reviewQuality，真实报告返回后再判断效果；不强制全套工具，长期确认及采用由作者决定。"}\n\n${CONTEXT_READING_INSTRUCTIONS}\n\n${TOOL_PROGRESS_INSTRUCTIONS}\n\n${decision ? `${POST_EDIT_INSPECTION_INSTRUCTIONS}\n\n${AGENT_STRATEGY_INSTRUCTIONS}\n\n${DELIVERY_GOAL_STATUS_INSTRUCTIONS}\n\n${QUALITY_DELIVERY_REVIEW_INSTRUCTIONS}` : ""}\n\n${ordinary ? CONVERSATION_ANALYSIS_INSTRUCTIONS : ""}\n\n${ordinary ? SCOPED_REVISION_INSTRUCTIONS : ""}\n\n检索与引用真实来源，区分事实、候选与推测。目标歧义才询问，未读范围说明限制；不要声称候选已采用或仅凭规划预测已实现。`, { internal: Boolean(decision || task.inputManifest.authorIntentBinding), binding: handoff,
    background: { candidateInstruction: task.inputManifest.candidateInstruction ?? null, focus: conversationFocusInstructions(focus) } });
}
