import { createHash } from "node:crypto";
import { authorIntentError } from "../shared/author-input-budget.mjs";
import { qualityReaderInstructions, QUALITY_READER_INTENT } from "./quality-notebook.mjs";
import { CHAPTER_LENGTH_INSTRUCTIONS } from "../shared/chapter-writing-policy.mjs";
import { CONTEXT_READING_INSTRUCTIONS, TOOL_PROGRESS_INSTRUCTIONS } from "./context-reading-contract.mjs";
import { chapterStructureHandoff, chapterStructurePrompt } from "./chapter-structure-prompt.mjs";

export const SCOPED_REVISION_INSTRUCTIONS = '明确整章重写先核对章节身份，request_candidate_task(operation=generateChapter,target.scope={type:"wholeChapter"})；局部改稿先完整读当前原稿，按正文sceneGroups稳定id定位范围，scope={type:"selectedScenes",sourceRevisionId,groupIds}；段落/文字修改增加bodyRanges=[{groupId,start,end}]，Unicode半开区间外逐字不变。readScope单独约束阅读，修改范围不限制必要阅读，只选作者授权场景，不猜规划sceneId或默认整章。歧义才询问，不要求作者填写ID。所有writer起草前附target.chapterAcceptance={requirements:[{id,text}],length:{requirement,minCharacters,maxCharacters}}，模型按作者要求解释标准，修订复用；verification={method:"direct"|"independent",reason}由模型选择适用核验方式，明确要求独立审阅时选independent；无篇幅限制用null。desiredOutcomes=[{id,criterion,groupIds}]仅selectedScenes可选额外具体效果，遵守工具字段界限，不免除有证据的结果检查，writer不改标准。修复讨论仍须独立范围审查。完整读回并提交章节，只改所选body，其他场景原文、身份、顺序、locked、标题和sourcePlanRefs保持；不增删场景或解除锁定。提交完整sceneGroups而非局部patch，省略重复body，MCP按工具契约派生全文。缺资料继续分页或说明阻碍，不凭空重造。来源过期不擅换版本扩权。保存后同Work模型可选verifyChapter直接核验（先主任务完整读原要求及当前稿，chapterReview提交outcomes和uncoveredRequirements），或选真实writer的assessRevisionOutcome独立核验；read_revision_outcome读当前报告，未通过或篇幅不符则原范围修订再按适用合同复核，知识工具按需选，候选不等于采用。';

export const STORY_QUERY_INSTRUCTIONS = '人物同名或身份问题先用 query_story 的 {type:"resolveEntity",name:"原文姓名"}，读取 results 中的源主张、canonicalId，以及 coverage.identityRevisionId/identityDecisions/ambiguousNames。identityDecisions 仅是该修订下与返回实体相关的作者已确认同一或不同决议；没有决议、截断或名字相同都不证明同一/不同，候选主张不是作者确认。只核对时直接回答，不登记合并、改写或确认；遵守任务范围，不能借此读取作者未来身份。\nquery_story/queryPlan 必须显式指定 ranking。先用 {type:"literal"}，自主把自然语言问题拆成正文检索词、身份、时间和关系，不把整句问题当作唯一字面检索词。需要按含义重排时，保留原 question/steps/byteBudget，仅改 ranking 为 {type:"modelSemantic",baseReportHash:原 literal 报告的 contentHash,judgments:[{resultId,score,rationale}]}。对原报告每个 resultId 一一给出0–1000相关性候选分数和不超过256 UTF-8字节的理由，包括相反主张；不能漏项、添加隐藏来源或上传自造报告。整个计划最多9KB，超限先缩小原计划重新查询。来源过期/hash变化时停止复用；空结果或截断不是不存在，不把排序当成扩大召回、事实确认或全局语义认证。\n字面检索不足时，可用 {type:"passages",cursor:null,maxChars:160,limit:2} 无关键词逐页读取授权正文，按含义寻找证据。保持问题和步骤ID，后续用返回 nextCursor；引用 evidence 字典的版本、路径和字符锚点。pageComplete=false 时不得跳页，应从原 cursor 降低 limit/maxChars 重读。endOfScopeFromCursor 仅是该游标之后到末尾，不证明之前已读或全书语义完成；遵守模型预算，停下时说明已读范围和不足。\n持久阅读时，根 Agent 先 read_reading_progress：checkpoint=null 才从首段开始；有效 resumePlan 与 version 原样续读。每页实际阅读后，用 checkpoint_reading 提交该 literal 单步骤计划、报告 contentHash（reportHash）、expectedVersion 和最多512 UTF-8字节的候选摘要；它不保存小说内容或确认事实。只有保存成功才使用返回的新 resumePlan/version；响应丢失时先重试完全相同的请求，不跳页。分页截断不能保存，应缩小同页重读。旧进度 stale 或当前问题不同则报告不能续用，不改写游标、不重置预算；需要新的分析任务。恢复新 Attempt 会重新授权游标，同一 WorkStep 最多10000页；摘要是模型声明，连续到末尾也不等于语义分析完整。子 Agent 不能读写根任务检查点。';

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
  return value;
}

// Runtime freezes these counterparts; Host separately verifies persisted Work membership.
function workAuthorSource(task) {
  const manifest = task.inputManifest, target = task.target;
  const decision = manifest.workDecision, action = manifest.workDecisionAction;
  if (!decision && !action) return false;
  const id = value => typeof value === "string" && value.trim() === value && value.length > 0
    && Buffer.byteLength(value) <= 160 && !/[\u0000-\u001f\u007f-\u009f]/u.test(value);
  const keys = action
    ? ["workTaskId", "originTaskId", "sourceTaskId", "rootThreadId", "decisionStepId", "decisionTaskId", "actionKey", "operation", "sourceRevisionId"]
    : ["workTaskId", "originTaskId", "sourceTaskId", "rootThreadId", "decisionStepId", "decisionTaskId"];
  const binding = action ?? decision;
  const declared = target?.[action ? "workDecisionAction" : "workDecision"];
  const declaredKeys = action ? ["workTaskId", "decisionStepId", "decisionTaskId", "actionKey"]
    : ["workTaskId", "originTaskId", "sourceTaskId", "fingerprint"];
  if ((decision && action) || !binding || !declared
    || Object.keys(binding).length !== keys.length || keys.some(key => !Object.hasOwn(binding, key))
    || Object.keys(declared).length !== declaredKeys.length || declaredKeys.filter(key => key !== "fingerprint").some(key => declared[key] !== binding[key])
    || (!action && (typeof declared.fingerprint !== "string" || !/^[a-f0-9]{64}$/.test(declared.fingerprint)))
    || ["workTaskId", "rootThreadId", "decisionStepId", "decisionTaskId"].some(key => !id(binding[key]))
    || ["originTaskId", "sourceTaskId"].some(key => binding[key] !== null && !id(binding[key]))
    || !id(target.resumeStepId)
    || (action ? binding.operation !== task.operation || !id(binding.actionKey)
      || (binding.sourceRevisionId !== null && !id(binding.sourceRevisionId))
      || target.resumeStepId === binding.decisionStepId
      : task.operation !== "advisory" || binding.decisionTaskId !== task.id
        || binding.rootThreadId !== task.threadId || binding.decisionStepId !== target.resumeStepId)) return false;
  if (target.runId === undefined) {
    const author = manifest.authorIntentBinding;
    return binding.originTaskId !== null && !manifest.generationAuthorIntentBinding && author && Object.keys(author).length === 3
      && author.sourceTaskId === binding.originTaskId && author.actionId === null
      && Number.isSafeInteger(author.intentRevision) && author.intentRevision >= 0;
  }
  const generation = manifest.generationAuthorIntentBinding;
  return !manifest.authorIntentBinding && generation && Object.keys(generation).length === 4
    && id(generation.runId) && generation.runId === target.runId
    && id(generation.authorMessageId) && generation.authorMessageId === manifest.authorMessageId
    && Number.isSafeInteger(generation.intentGeneration) && generation.intentGeneration >= 0
    && generation.intentGeneration === target.intentGeneration
    && Number.isSafeInteger(generation.chapterNumber) && generation.chapterNumber > 0
    && (target.chapterNumber === undefined || generation.chapterNumber === target.chapterNumber);
}

// Native verifies the same WorkStep and prior Attempt before issuing this
// resume binding. Host verifies persisted Work membership before using it.
function resumedAuthorSource(task) {
  const target = task.target, manifest = task.inputManifest;
  const binding = manifest.authorIntentBinding;
  const id = value => typeof value === "string" && /^[A-Za-z0-9_-]{1,160}$/.test(value);
  return binding && Object.keys(binding).length === 3
    && id(binding.sourceTaskId) && binding.actionId === null
    && Number.isSafeInteger(binding.intentRevision) && binding.intentRevision >= 0
    && binding.intentRevision <= 16 && id(target?.resumeStepId)
    && !["requestedActionId", "requestedByTaskId", "runId", "workDecision", "workDecisionAction"].some(key => Object.hasOwn(target, key))
    && !["generationAuthorIntentBinding", "workDecision", "workDecisionAction"].some(key => Object.hasOwn(manifest, key));
}

export function taskMethodText(task) {
  const binding = task?.inputManifest?.creativeCapabilityBinding;
  if (!binding || binding.taskId !== task.id || binding.inputRevision !== task.inputRevision || binding.operation !== task.operation) throw new Error("任务缺少有效的冻结方法绑定");
  const { bindingDigest, ...content } = binding;
  const digest = (value) => createHash("sha256").update(value).digest("hex");
  if (bindingDigest !== digest(JSON.stringify(canonical(content)))) throw new Error("冻结方法绑定摘要不匹配");
  if (!Array.isArray(binding.selectedMethods) || binding.selectedMethods.length < 1 || binding.selectedMethods.length > 3) throw new Error("冻结方法数量无效");
  const methods = binding.selectedMethods.map((method) => {
    if (typeof method.text !== "string" || Buffer.byteLength(method.text, "utf8") > 12_000 || method.digest !== digest(method.text)) throw new Error("冻结方法正文或摘要无效");
    return method.text;
  }).join("\n\n");
  if (Buffer.byteLength(methods, "utf8") > 20_000) throw new Error("冻结方法超过总预算");
  return methods;
}

export function taskAuthorPrompt(task) {
  const manifest = task?.inputManifest;
  const imageOnly = task?.operation === "advisory" && task.target?.type === "edition"
    && authorIntentError(manifest?.authorIntent, { imageAssetRevisionIds: task.target.imageAssetRevisionIds }) === null;
  if (typeof manifest?.authorIntent !== "string" || !manifest.authorIntent.trim() && !imageOnly) throw new Error("任务缺少冻结的作者要求");
  if (task.operation === "reviewPlanningSeat" || task.target?.reviewRunId !== undefined || manifest.planningReviewAuthorIntentBinding !== undefined) {
    const binding = manifest.planningReviewAuthorIntentBinding;
    const keys = ["runId", "authorMessageId", "role", "stepId", "sourcePacketHash", "sourceTaskId", "sourceIntentRevision"];
    const target = task.target;
    const validId = (value) => typeof value === "string" && /^[A-Za-z0-9_-]{1,160}$/.test(value);
    if (!binding || Object.keys(binding).length !== keys.length || keys.some((key) => !Object.hasOwn(binding, key))
      || !validId(binding.runId) || !validId(binding.authorMessageId)
      || !["structure", "character", "experience"].includes(binding.role)
      || !/^[a-f0-9]{64}$/.test(binding.sourcePacketHash)
      || task.operation !== "reviewPlanningSeat" || target?.type !== "artifact"
      || !["outline", "plan"].includes(target.artifactKind) || !validId(target.revisionId)
      || Object.keys(target).length !== 6 || target.reviewRunId !== binding.runId || target.role !== binding.role
      || target.resumeStepId !== binding.stepId || binding.stepId !== `review-seat-${binding.runId}-${binding.role}`
      || binding.authorMessageId !== manifest.authorMessageId || binding.sourcePacketHash !== manifest.sourcePacketHash
      || (binding.sourceTaskId === null
        ? binding.sourceIntentRevision !== null || Array.from(manifest.authorIntent).length > 4_000
        : !validId(binding.sourceTaskId) || !Number.isSafeInteger(binding.sourceIntentRevision)
          || binding.sourceIntentRevision < 0 || Buffer.byteLength(manifest.authorIntent) > 24_000)
      || ["authorIntentBinding", "candidateInstruction", "generationAuthorIntentBinding"].some((key) => Object.hasOwn(manifest, key))) {
      throw new Error("规划任务缺少有效的冻结作者来源");
    }
    return manifest.authorIntent;
  }
  if (manifest.candidateInstruction === undefined) return manifest.authorIntent;
  const generation = manifest.generationAuthorIntentBinding;
  const generationRepair = task.operation === "generateChapter"
    && typeof generation?.runId === "string" && generation.runId.length > 0
    && generation.runId === task.target?.runId
    && typeof generation.authorMessageId === "string" && generation.authorMessageId.length > 0
    && generation.authorMessageId === manifest.authorMessageId
    && Number.isSafeInteger(generation.intentGeneration) && generation.intentGeneration >= 0
    && generation.intentGeneration === task.target?.intentGeneration
    && Number.isSafeInteger(generation.chapterNumber) && generation.chapterNumber > 0
    && generation.chapterNumber === task.target?.chapterNumber;
  if (typeof manifest.candidateInstruction !== "string" || !manifest.candidateInstruction.trim()
    || (!manifest.authorIntentBinding?.actionId && !generationRepair && !workAuthorSource(task) && !resumedAuthorSource(task))) throw new Error("候选任务缺少作者要求来源绑定");
  return `作者完整要求：${manifest.authorIntent}\n\n本项候选修改：${manifest.candidateInstruction}`;
}

export function taskExecutionPrompt(task) {
  if (task?.operation === "reviewQuality") return QUALITY_READER_INTENT;
  if (task?.operation === "assessChapterStructure") return chapterStructurePrompt(task);
    const authorPrompt = taskAuthorPrompt(task);
    const instructions = {
      generateBrief: '只建立本作品创作简报候选，使用 save_brief。payload 为 {premise:string,promise?:string,stakes?:string,endingDirection?:string}：核心命题、读者承诺、核心代价、结局方向；不得写入 title/logline/chapterRange/draftSpec/content/chapterNumber 等其他字段，不把章节正文伪装成简报。',
      generateOutline: '只生成全书层级大纲候选，使用 save_outline；payload.nodes 按工具 schema 提供完整节点字段，保留作者要求及受保护结构，不在本任务写章纲或章节正文。',
      generatePlan: '只生成近章章纲候选，使用 save_chapter_plans。target.planFromChapter 存在时从该章起按当前创作目标选择需要规划的章节范围，不固定为三章；未提供时按作者要求和实际章节核对起章号，不伪造冻结字段。payload 必须使用 chapters 数组，不用 plans/chapterPlans 包装。核对本轮意图、已发生正文和当前规划，保留一致内容及稳定 id，仅修改冲突部分；每章 chapterPlanId 非空，场景和节拍使用稳定 id，场景列出 expectedEffects（稳定 id、dimension、description、commitmentRefs；无关联为空数组，每效果最多4条承诺引用、每场景最多12项效果、全计划承诺引用总数最多64项）。不得在本任务生成章节正文，不把预期效果当作已发生事实。',
      generateChapter: '只生成或修改冻结 target 指定章节与场景范围的完整正文，使用 save_draft_checkpoint，章节身份与保护范围原样来自 target；不得写其他章节或将正文存入简报、大纲、章纲。',
      generatePropertyRules: '只生成本任务绑定的故事属性结构规则候选，使用 save_property_rules，不确认属性值或修改正文、规划。',
      modelKnowledge: '只分析绑定的已保存正文，提取有原文来源的候选故事知识并使用 publish_knowledge_batch 保存。正文修改由写作任务负责，不在本分析任务重新改稿；缺少来源或保存失败时如实说明，不宣称分析完成。',
      extractCommitments: '只读取冻结 target.sourceRevisionId 的本章正文并提取叙事承诺提案，使用 submit_commitment_extraction；声明实际阅读范围，不生成正文或建立、确认正式承诺。',
      assessSceneOutcome: '只评估冻结章节的规划场景与正文实际效果，使用 submit_scene_assessment 保存候选分析；不改写正文、规划、故事设定或承诺状态。',
      reviseCommitment: '只调整冻结 target.commitmentId 的叙事承诺候选，使用 submit_commitment_candidate；不确认正式承诺或生成正文。',
    };
    const instruction = instructions[task.operation];
    if (!instruction) return authorPrompt;
    return `当前唯一任务：${instruction} 保存成功后调用 complete_task 并结束本任务；后继由同Work决策Agent根据真实产物选择，本子任务只执行其授权操作。以下作者完整要求仅作为本项职责的背景与约束，不是要求本任务执行整个创作流程。不得因其他保存工具不可用而把另一类产物放入本工具。\n\n原作者要求（背景，含本项修改要求）：\n${authorPrompt}`;
  }

export function createTaskBoundHandoff(projectId, workspace, task) {
  if (!task || typeof task !== "object" || !task.id || !task.threadId) throw new Error("Runtime 任务绑定无效");
  if (!workspace || workspace.revision !== task.inputRevision) throw new Error("任务输入版本与当前工作区不一致，请重新发起生成");
  taskMethodText(task);
  const binding = task.inputManifest.creativeCapabilityBinding;
  // Source data is fetched once through read_context, not duplicated in instructions.
  const { authorIntent: _authorIntent, candidateInstruction: _candidateInstruction,
    revisionRepairInput: _revisionRepairInput, ...inputManifest } = task.inputManifest;
  if (binding.projectId !== projectId) throw new Error("冻结方法不属于当前作品");
  return {
    task: {
      taskId: task.id,
      projectId,
      threadId: task.threadId,
      operation: task.operation ?? null,
      purpose: task.purpose,
      expectedRevision: task.inputRevision,
      requiredOutputKind: task.requiredOutputKind ?? null,
      target: task.target ?? null,
      inputManifest: { ...inputManifest, creativeCapabilityBinding: { ...binding, selectedMethods: binding.selectedMethods.map(({ text: _text, ...metadata }) => metadata) } },
    },
  };
}

export function taskBoundInstructions(handoff) {
  if (handoff.task.operation === "reviewQuality") return qualityReaderInstructions(handoff.task);
  if (handoff.task.operation === "assessChapterStructure") return chapterStructureHandoff(handoff);
  const operation = handoff.task.operation;
  const contracts = {
    generateBrief: '修改前完整读取冻结原简报；其他规划或正文只按作者问题读取。只保存简报候选。',
    generateOutline: '完整读取修改范围的原大纲和受保护结构。book.purpose 写全书主线冲突与目标，summary 写起因、推进、转折、高潮、收束的因果脉络；volume 写本阶段贡献、事件与边界，不重复全书；unit 写具体安排。局部范围不强写全书，未定结局不虚构定案。',
    generatePlan: '完整读取冻结近章规划及相关大纲路径；依据已发生正文调整近章，不把未来安排当事实，不自行更换起章号。保留稳定章纲、场景和节拍身份。',
    generatePropertyRules: '修改前完整读取冻结规则基准；只保存规则候选，不确认属性值或事实。',
    generateChapter: '完整读取本章冻结原稿、绑定场景计划和保护范围；desiredOutcomes是作者已批准标准，不得增删或改变；没有原稿才首写。时间、金额、持物及因果修改先读相关前文，整理必要事实与发生顺序再写；不以章纲替代已发生正文。保存回执后complete_task交给同Work新决策读回当前稿逐项对照；writer输入仍是冻结原稿，不据其旧文判断保存失败。selectedScenes 必须提交全部原组，只改选中组正文，未选与锁定组、身份、顺序、标题和sourcePlanRefs不变，不重造未读内容。整章chapterComplete有绑定章纲时至少一个非空组须用本章合法sceneId；过渡可null，不借映射宣称效果实现。模型正文只提交body或完整sceneGroups之一，MCP派生canonical body；校验失败在同任务预算内纠正完整payload，不换目标/CAS。',
    reviewPlanningSeat: '完整读取冻结评审资料与明确范围，按本席位角色给候选报告；不生成作品或扩大范围。',
    modelKnowledge: '完整读取本批次冻结正文与必要实体身份；只提取有精确原文依据的候选实体、事实、事件、状态、关系和认知，不编造时间、身份或前提。每条assertions都必须包含kind、subjectId、predicate、value、evidence，包括relation和epistemic；按具体kind补充schema要求的其他字段，不能只修正上一次指出的字段而遗漏其他必填项。已有目录ID与跨源引用原样保留，前提只来自本批次或严格前文章节；证据优先提交已读原文中的唯一短句 {quote}，由Runtime解析位置，不必为每条证据调用resolve_story_evidence；仅引文重复或需要明确位置时定位一次并复用。不得自行估算Unicode字符位置。候选不等于作者确认的Canon，确认事件不自动确认效果。报告实际范围；发布committed后立即complete_task，不再查询或重发。参数含绑定身份总量不超过8192UTF8B，证据用必要短quote；超限停止说明，不截断必填数据或重复发布。',
    extractCommitments: '只读冻结sourceRevisionId原文，examinedRanges按整章Unicode位置只声明实际阅读。连续创作要求整章覆盖；原文不足不能虚构范围。候选摘录经resolve_story_evidence核对唯一锚点；空结果不证明没有伏笔。完整报告一次原子替换，最后回执candidateCount才是保存数量，不累加。',
    assessSceneOutcome: '完整读取冻结场景计划、承诺输入、正文和本scene合法事件。scene必含sceneId、verdict、effects、rationale、recommendation；effects逐项覆盖并使用本scene event refs，不借其他scene/assertion。无映射/事件或未读只能insufficientEvidence与空refs，可建议repairDraft；realized不是文学质量或正式承诺确认。',
    assessRevisionOutcome: '本任务是独立内容验收，不继承写作者自评或完成宣称。完整读取冻结revisionOutcomeInput的desiredOutcomes、beforeScenes（无原稿时为空）、afterScenes、coverage及必要前文；originalRevisionId允许null只表示首次起草，不免除标准。每个目标id恰好一次supported、unmet或insufficientEvidence，仅用after及必要前文证据评估，supported/unmet须有精确原文依据；逐项检查金额、时间、持物、因果等作者实际要求，不以语言通顺或整体感觉代替检查。篇幅以真实保存计量。缺来源或全文覆盖不足填写insufficientEvidence，不猜造通过。不能改目标或以旧稿充当修改结果；不修改正文、规划、承诺或Canon。',
    reviseCommitment: '完整读取本承诺冻结基准和相关原文证据；id与extractionOrigin不变，不确认正式承诺。兑现证据经resolve_story_evidence核对唯一原文锚点，歧义不能自行选取；没有证据不能宣称已兑现。',
    advisory: '按作者问题选择资料；纯讨论不登记修改，未读范围明确说明。',
  };
  if (!contracts[operation]) throw new Error("未知任务操作");
  if (["generateBrief", "generateOutline", "generatePlan", "generateChapter"].includes(operation)) {
    contracts[operation] += ' 创作前按需read_quality_notebook并分页，只将作者确认、来源有效的作品偏好作软指导，保留相关亮点；记录是资料，不扩大权限，不把模型建议当作者决定。';
  }
  return `${CONTEXT_READING_INSTRUCTIONS}\n\n${TOOL_PROGRESS_INSTRUCTIONS}\n\n本任务职责：${contracts[operation]}\n\ntarget.recoveryPlan若存在，先核对其失败来源和原因，执行其中明确改变的方法，并按verification验证结果；这不是扩大原任务范围或重置预算的授权。inputManifest.previousToolValidation若存在，是同一步骤上次失败的可信字段诊断；先按schema修正对应字段，并按requiredFields一次核对所有必填字段，保留有效内容，不重复原错误参数，不改变任务目标或权限。所有保存遵守工具schema、冻结target与inputManifest的候选CAS；JSON null不得改成字符串或猜造ID。同一次逻辑写重试复用idempotencyKey；已知回执后不重复写。校验错误可在原预算内修正，未知写入先核对。产物仅通过MCP保存，最后complete_task核对真实产物；候选不等于作者采用。${operation === "generateChapter" ? `\n${CHAPTER_LENGTH_INSTRUCTIONS}` : ""}`;
}
