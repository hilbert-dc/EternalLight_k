import { createHash } from "node:crypto";

export const SCOPED_REVISION_INSTRUCTIONS = '局部改稿先读取当前正文及 sourceRevisionId，定位 sceneGroups 的稳定 id（不是规划 sceneId）。request_candidate_task(operation=generateChapter) 的 target 可含 scope={type:"selectedScenes",sourceRevisionId,groupIds:[正文场景id]}，最多16项；只能选择作者要求修改的场景，歧义先询问，不能默认为整章重写。可依据作者要求提出 desiredOutcomes:[{id,criterion,groupIds}]，仅 selectedScenes 的获批行动支持此目标；共1–4项，id 为1–64位 ASCII 字母、数字、_ 或 - 且唯一，criterion 非空且最多500个 Unicode 字符，groupIds 非空、唯一、最多16项且属于所选范围。criterion 应描述可在正文中核对的具体效果，不能承诺模型已证实或作者已同意；作者在已有批准/范围确认界面看见并批准后冻结，Writer 不得增删或改变标准。未提供 desiredOutcomes 的任务没有目标评估。执行此范围时读取并提交完整章节，只修改选中场景的 body，其他场景、顺序、id、sceneId、locked、标题和 sourcePlanRefs 原样保留；body 按 sceneGroups 的 body.trim() 用 \\n\\n---\\n\\n 拼接。不可新增/删除场景或解除锁定。上下文不完整则继续读取授权原稿或停止说明，不能凭空重造缺失内容。来源过期应请求新任务，不能换 sourceRevisionId 扩大权限；保存后建模仍由现有后续任务完成，含冻结目标时再由独立任务评估改稿结果，候选不等于采用。';

export const STORY_QUERY_INSTRUCTIONS = 'query_story/queryPlan 必须显式指定 ranking。先用 {type:"literal"}，自主把自然语言问题拆成正文检索词、身份、时间和关系，不把整句问题当作唯一字面检索词。需要按含义重排时，保留原 question/steps/byteBudget，仅改 ranking 为 {type:"modelSemantic",baseReportHash:原 literal 报告的 contentHash,judgments:[{resultId,score,rationale}]}。对原报告每个 resultId 一一给出0–1000相关性候选分数和不超过256 UTF-8字节的理由，包括相反主张；不能漏项、添加隐藏来源或上传自造报告。整个计划最多9KB，超限先缩小原计划重新查询。来源过期/hash变化时停止复用；空结果或截断不是不存在，不把排序当成扩大召回、事实确认或全局语义认证。\n字面检索不足时，可用 {type:"passages",cursor:null,maxChars:160,limit:2} 无关键词逐页读取授权正文，按含义寻找证据。保持问题和步骤ID，后续用返回 nextCursor；引用 evidence 字典的版本、路径和字符锚点。pageComplete=false 时不得跳页，应从原 cursor 降低 limit/maxChars 重读。endOfScopeFromCursor 仅是该游标之后到末尾，不证明之前已读或全书语义完成；遵守模型预算，停下时说明已读范围和不足。\n持久阅读时，根 Agent 先 read_reading_progress：checkpoint=null 才从首段开始；有效 resumePlan 与 version 原样续读。每页实际阅读后，用 checkpoint_reading 提交该 literal 单步骤计划、报告 contentHash（reportHash）、expectedVersion 和最多512 UTF-8字节的候选摘要；它不保存小说内容或确认事实。只有保存成功才使用返回的新 resumePlan/version；响应丢失时先重试完全相同的请求，不跳页。分页截断不能保存，应缩小同页重读。旧进度 stale 或当前问题不同则报告不能续用，不改写游标、不重置预算；需要新的分析任务。恢复新 Attempt 会重新授权游标，同一 WorkStep 最多10000页；摘要是模型声明，连续到末尾也不等于语义分析完整。子 Agent 不能读写根任务检查点。';

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
  return value;
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
  if (typeof manifest?.authorIntent !== "string" || !manifest.authorIntent.trim()) throw new Error("任务缺少冻结的作者要求");
  if (task.operation === "reviewPlanningSeat" || task.target?.reviewRunId !== undefined || manifest.planningReviewAuthorIntentBinding !== undefined) {
    const binding = manifest.planningReviewAuthorIntentBinding;
    const keys = ["runId", "authorMessageId", "role", "stepId", "sourcePacketHash"];
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
      || Array.from(manifest.authorIntent).length > 4_000
      || ["authorIntentBinding", "candidateInstruction", "generationAuthorIntentBinding", "domainTaskSource", "domainInstruction"].some((key) => Object.hasOwn(manifest, key))) {
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
    || (!manifest.authorIntentBinding?.actionId && !generationRepair)) throw new Error("候选任务缺少作者要求来源绑定");
  return `作者完整要求：${manifest.authorIntent}\n\n本项候选修改：${manifest.candidateInstruction}`;
}

export function taskExecutionPrompt(task) {
  const manifest = task?.inputManifest;
  const source = manifest?.domainTaskSource;
  if (!source && manifest?.domainInstruction === undefined && task?.target?.modelingIntent === undefined) return taskAuthorPrompt(task);
  const validId = (value) => typeof value === "string" && value.length > 0 && value.trim() === value
    && Buffer.byteLength(value, "utf8") <= 160 && !/[\u0000-\u001f\u007f-\u009f]/u.test(value);
  const keys = ["type", "workTaskId", "stepId", "batchId", "objectId", "sourceRevisionId", "chapterId", "intentGeneration"];
  const target = task?.target;
  if (!source || typeof source !== "object" || Object.keys(source).length !== keys.length
    || keys.some((key) => !Object.hasOwn(source, key)) || source.type !== "modelingOutbox"
    || keys.slice(1, -1).some((key) => !validId(source[key]))
    || !Number.isSafeInteger(source.intentGeneration) || source.intentGeneration <= 0
    || task.operation !== "modelKnowledge" || source.stepId !== target?.resumeStepId
    || source.batchId !== target?.batchId || source.chapterId !== target?.chapterId
    || target.modelingIntent !== true || Object.keys(target).length !== 4
    || source.objectId !== `draft-${source.chapterId}`
    || typeof manifest.domainInstruction !== "string" || !manifest.domainInstruction.trim()
    || Buffer.byteLength(manifest.domainInstruction, "utf8") > 2_000
    || ["authorIntent", "authorMessageId", "authorIntentBinding", "candidateInstruction", "generationAuthorIntentBinding", "planningReviewAuthorIntentBinding"].some((key) => Object.hasOwn(manifest, key))) {
    throw new Error("任务缺少有效的冻结领域来源");
  }
  return `领域任务要求（系统派发，不是作者原话）：${manifest.domainInstruction}`;
}

export function createTaskBoundHandoff(projectId, workspace, task) {
  if (!task || typeof task !== "object" || !task.id || !task.threadId) throw new Error("Runtime 任务绑定无效");
  if (!workspace || workspace.revision !== task.inputRevision) throw new Error("任务输入版本与当前工作区不一致，请重新发起生成");
  taskMethodText(task);
  const binding = task.inputManifest.creativeCapabilityBinding;
  // Source data is fetched once through read_context, not duplicated in instructions.
  const { authorIntent: _authorIntent, candidateInstruction: _candidateInstruction, domainInstruction: _domainInstruction,
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
  const stateContract = handoff.task.operation === "modelKnowledge"
    ? '\n\n属性状态使用第五种候选 kind="state"，不与普通 assertion、event、relation 或 epistemic 混用。必填 subjectId、predicate（最多80字节）、value、stateSchema、validFrom、evidence；validFrom 是世界时间整数或显式 null，不使用 storyTime。stateSchema={cardinality:"single"|"multiple",valueType:{type:"boolean"|"integer"|"text"|"entity"}}；entity 类型还必须给 valueType.kind（已有实体种类），value 只含 {entityId} 并引用同批次正确种类实体；其他 value 为对应布尔、整数或非空文字（最多512 UTF-8字节），不可用 null、浮点、数组或任意 JSON。区间结束可给 validUntilExclusive 和同源 validUntilEvidence，但必须有已知 validFrom 且结束严格晚于开始；结束只表示该来源主张不再适用，不能推导反值。没有原文支持时不提取，不把事件或未来规划当成状态。single/multiple 是 modelDeclared 解释，不是作者确认的全局世界规则，规则分歧必须保留；不可通过排他声明覆盖其他来源。不得添加上述字段之外的字段。'
    : "";
  const stateConsumption = '带区间的属性 state 不进入通用 stateFacts；确认仅确认有源主张，不抹掉有效区间。需要属性时使用 query_story 的 state 步骤（显式 perspective="world"、相关 entityId/predicate，时间已知时 worldTime），检查 stateSchemaAuthority、confirmationState、coverage 和冲突。authorConfirmed 只表示该查询应用作者采用的结构规则，不能把候选值当成确认事实；检查 propertyRuleAssessment、规则修订和 effectiveStateSchema。typeMismatch 不可强制转型，requiresInterpretation 的自然语言例外不可视为不存在。未知时间不是永久有效，区间结束不是反值。遵守任务的 Writer/POV/章节限制，不从 typedStateFacts 缺口推断不存在，也不请求扩大为作者全局规则视角。';
  const eventConsumption = '事件不进入通用 stateFacts；typedEventFacts 缺口不是不存在。经 query_story 的 state 步骤按事件或参与者 entityId、事件类型 predicate、授权视角和时间查询，保留 modelingBatchId、sourceRevisionId 与 assertionIndex，按 value 中的 sourceBatchId/assertionIndex 精确来源解释主张编号。确认事件不自动确认前提/效果；空引用表示尚未解释，duration 不终止属性有效期，不从时间先后推出因果。';
  const eventContract = handoff.task.operation === "modelKnowledge"
    ? '\n\n事件 kind="event" 必須引用同批次 kind="event" 实体作为 subjectId，predicate 为事件类型（最多80字节）。storyTime 必填世界时间整数或 null；value 只含 {participants:[{role,entityId}],preconditions:[{sourceBatchId:null|批次ID,assertionIndex}],effects:[{sourceBatchId:null,assertionIndex}],untilExclusive:整数|null}。参与者最多16项；角色 actor/recipient/beneficiary 引用人物或组织，affected 引用人物/组织/地点/物品，location 引用地点，instrument/item 引用物品，witness 引用人物；同角色同实体不重复。前提与效果各最多16项来源限定的非事件主张引用，sourceBatchId 和 assertionIndex 两字段必填，null 表示本批次；序号为小于100的零基整数，批次ID最多160 UTF-8字节。同一来源与序号不可重复或跨两侧复用。前提可引用同作品当前有效且严格前文章节：先用授权 query_story 查询，照抄 modelingBatchId 和 assertionIndex，不猜来源，不借用未来或其他作品。效果仅限本批次。主体或认知持有者须精确匹配参与者；本批次关联证据和结束证据在事件 evidence 范围内，跨源前提独立核对来源原文。结束时间必须晚于已知开始；未知不编造，空引用不证明无影响。仅有普通观察时用 assertion。事件解释不执行状态变化，确认事件不自动确认效果。'
    : "";
  const revisionContract = handoff.task.target?.scope?.type === "selectedScenes" ? SCOPED_REVISION_INSTRUCTIONS : "";
  const protectedScenes = handoff.task.operation === "generateChapter"
    ? '所有写作范围均须保留作者工作依据中 locked=true 的场景原位置、完整内容和元信息，不得解锁、删除或重排；整章生成不豁免。未锁或新场景显式 locked=false，不得自行增加锁定。场景 id 唯一，body 与 sceneGroups 正文按既有分隔符拼接一致。无法在保护范围内实现要求时说明冲突，不绕过保护。'
    : "";
  const queryInstructions = handoff.task.operation === "assessRevisionOutcome"
    ? 'query_story/queryPlan 必须显式指定 ranking，先用 {type:"literal"}。text 每步检索一个连续子串；需要语义重排时保持 question/steps/byteBudget，用 {type:"modelSemantic",baseReportHash:原报告contentHash,judgments:[{resultId,score,rationale}]} 覆盖每个原 resultId，score 为0–1000，rationale 最多256 UTF-8字节。计划最多9KB，证据身份从共享 evidence 字典照抄。所有检索仅补充冻结候选章节证据，不能读取完整历史原稿、未来正文或扩大任务范围；结果缺失、截断或语义排序都不等于目标达成或未达成。原稿比较只用 revisionOutcomeInput 的 beforeScenes，证据只用实际改动的 afterScenes 正文。'
    : STORY_QUERY_INSTRUCTIONS;
  return `${queryInstructions}\n\n${operationInstructions(handoff)}\n\n${revisionContract}\n\n${protectedScenes}\n\n${stateConsumption}${stateContract}\n\n${eventConsumption}${eventContract}`;
}

function operationInstructions(handoff) {
  if (handoff.task.operation === "assessRevisionOutcome") {
    return `Novel Runtime 局部改稿目标评估任务绑定：\n${JSON.stringify(handoff, null, 2)}\n\n先 novel_workspace.read_context(purpose=critic,budget=24,byteBudget=60000)，原样传入任务身份和 expectedRevision。只比较 operationContext.revisionOutcomeInput 中冻结的 desiredOutcomes、beforeScenes、afterScenes 和 coverage；不请求完整历史原稿或未来章节。按作者批准的每条 criterion 判断，不重新定义目标，不把写作完成或知识建模完成视为目标达成；未改动场景不能作为改稿效果证据。覆盖不完整或证据不足时使用 insufficientEvidence，并在 reason 说明限制。调用 submit_revision_outcome，writerTaskId 照抄 target.writerTaskId；payload 严格为 {writerTaskId,chapterId,originalRevisionId,draftRevisionId,knowledgeBatchId,outcomes:[{id,status,reason,evidence:[{groupId,quote,start,end}]}]}，五个来源字段全部照抄 target，每个目标 id 恰好一次。status 只能 supported、unmet 或 insufficientEvidence；reason 非空且最多1000个 Unicode 字符；每项 evidence 最多4条，supported/unmet 必须有证据。证据 groupId 必须在该目标 groupIds 中且属于实际改动的新场景；quote 非空且最多1000个 Unicode 字符，精确匹配 afterScenes 中对应场景 body 的 start/end，使用场景正文内 Unicode 字符偏移，end 不含结束字符，不使用整章或 UTF-16 偏移。supported 表示正文证据支持该标准，unmet 表示正文证据显示未达到，均须说明证据与 criterion 的关系；空检索不证明未达到。首次 expectedCandidateRevisionId 使用 inputManifest 的值，后续使用保存返回 revisionId；同一逻辑写入重试保持 payload 和 idempotencyKey 不变。保存后 complete_task。只保存 revisionOutcome 候选分析，不修改正文、规划、承诺或 Canon，不调用写作/建议工具，不登记修复任务；子 Agent 遵守既有只读限制。`;
  }
  if (handoff.task.operation === "generatePropertyRules") {
    return `Novel Runtime 属性结构规则任务绑定：\n${JSON.stringify(handoff, null, 2)}\n\n先 novel_workspace.read_context(purpose=planner,budget=24,byteBudget=60000)，原样传入任务身份和 expectedRevision。operationContext.propertyRulesBase 仅描述任务捕获的规则基线，不含完整 payload。根任务用 read_property_rules(cursor=null) 开始，随后原样使用 nextCursor；核对 contentHash、byteRange 连续性和 total，将各页 payloadPart 原样拼接并解析 JSON，读到结尾后才基于完整基线调整。endOfBaselineFromCursor 不证明之前页已读或语义完整；页丢失重读相同 cursor，不换当前 head。不从截断正文猜测全书规则；保存为整份替换，保留作者未要求修改的属性与事件规则，歧义先询问。payload 必填 definitions 与 eventRules 两数组，每项必填 predicate（非空，最多80 UTF-8字节）、subjectKind(character/organization/location/item/proposition/event)、schema:{cardinality:single|multiple,valueType:{type:boolean|integer|text|entity}}、description（非空，最多512字节）、exceptions（最多8条非空文字，每条256字节）。entity 的 valueType 还必须包含 kind（同主体种类集合）。同一主体种类和属性键唯一，最多64条，整份 payload 16KiB；清空规则只有作者明确要求时使用两个空数组。eventRules 最多16条，eventType唯一且非空80字节；每项为 {eventType,description,exceptions,condition}，说明与例外同属性限额，每条4KiB。condition 为 {type:all|any,conditions:[...]}、{type:not,condition:...} 或 {type:state,role,subjectKind,predicate,quantifier:all|any,operator:eq|ne|lt|le|gt|ge,operand}，深度最多4（根0），最多31节点，每组1–8项。叶子引用本 payload 已声明的主体种类/属性，subjectKind须符合角色。角色 actor/recipient/beneficiary 允许 character|organization，affected 还允许 location|item，location 只允许 location，instrument/item只允许item，witness只允许character。operand 为 {type:boolean|integer|text,value} 或 {type:participant,role,quantifier:all|any}，类型匹配属性；lt/le/gt/ge只允许integer；规则整数常量限±9007199254740991，文字值非空且最多512字节；entity比较引用目标种类匹配的参与者角色，不使用硬编码实体ID。未知字段或脚本禁止；自然语言例外仍待解释，合法或采用表达式不证明条件成立，不执行状态变化。说明和例外不是代码，不编造数值或因果推演已完成，不执行 SQL/DDL，不确认任何属性值。调用 save_property_rules，首次 expectedCandidateRevisionId 来自 inputManifest，后续用返回 revisionId；重试保持原载荷与同一 idempotencyKey。输入过期立即停止，不改变基线。保存后 complete_task；仅报告模型提案已形成，正式生效仍需作者审阅采用。`;
  }
  if (handoff.task.operation === "assessSceneOutcome") {
    return `Novel Runtime 场景与承诺评估任务绑定：\n${JSON.stringify(handoff, null, 2)}\n\n先 novel_workspace.read_context(purpose=critic,budget=24,byteBudget=60000)，原样传入任务身份和 expectedRevision。读取 operationContext.scenePlanDraftLinks、boundCommitmentInputs、绑定正文及事件证据；不扩大到任意历史或未来章节。submit_scene_assessment 的 payload 包含 chapterId、planRevisionId、draftRevisionId、knowledgeBatchId、inputSnapshotId、intentGeneration（照抄 target）及 assessments。每个 sceneId 的 effectAssessments 精确覆盖原规划的全部 effectId，每项含 verdict(realized/deviated/insufficientEvidence)、rationale、actualEventRefs:[{modelingBatchId,assertionIndex}] 和 commitmentAssessments。realized 必须有该场景的正文事件；有事件不等于效果已实现。commitmentAssessments 必须一一覆盖该效果的 commitmentRefs，未绑定则显式 []；每项含 reference（原样照抄引用）、referenceState（照抄 boundCommitmentInputs）、outcome(advanced/partiallyFulfilled/fulfilled/contradicted/unchanged/insufficientEvidence)、rationale、actualEventRefs。依据原 fulfillmentCriteria 判断，advanced/partiallyFulfilled/fulfilled/contradicted 必须引用该效果 actualEventRefs 的非空子集，不可借用其他场景事件。referenceState 不是 current 时只能 insufficientEvidence，不能替换为新引用。场景汇总：任一效果 deviated 或承诺 contradicted 则 deviated，否则任一 insufficientEvidence 则 insufficientEvidence，否则 realized。场景含 rationale 与 recommendation(continue/repairDraft/replan/askAuthor)。首次提交使用 inputManifest.expectedCandidateRevisionId，后续使用返回 revisionId，重试复用 idempotencyKey；保存后 complete_task。只保存候选分析，不得修改正式承诺状态或 Canon；场景效果实现不等于整条承诺已兑现。`;
  }
  if (handoff.task.operation === "reviseCommitment") {
    return `Novel Runtime 承诺调整任务绑定：\n${JSON.stringify(handoff, null, 2)}\n\n先 read_context(purpose=critic,budget=24,byteBudget=60000)，再 read_commitment(commitmentId=task.target.commitmentId)，所有工具原样传入任务身份及 expectedRevision。它返回冻结基准，不依赖可选 Context Pack 是否列出该对象。只按作者指令修改这一个承诺；payload.id 不变，不生成新承诺，不修改 extractionOrigin。必要时 query_story/search_story 查当前正文证据，不能从计划、预测或不完整检索推断已经兑现。setupEvidenceRefs 和 fulfillmentEvidenceRefs 必须使用精确原文摘录及 Unicode start/end，fulfilled/partiallyFulfilled 必须有兑现证据，abandoned 必须给出 decisionReason。submit_commitment_candidate 的 payload 为完整 NarrativeCommitment；commitmentId 照抄 target。第一次提交使用 inputManifest.expectedCandidateRevisionId，后续检查点使用返回 revisionId；同一次重试复用 idempotencyKey。输入过期时停止，不自行切换目标或新基准。保存后 complete_task；只报告候选已生成，正式采用仍由作者确认。`;
  }
  if (handoff.task.operation === "extractCommitments") {
    return `Novel Runtime 叙事承诺提取任务绑定：\n${JSON.stringify(handoff, null, 2)}\n\n先调用 novel_workspace.read_context，原样传入绑定身份、expectedRevision，purpose=critic、budget=24、byteBudget=60000。只分析 relevantText 中 sourceRevisionId 等于 task.target.sourceRevisionId 的 content.body，不从计划或未来章节编造承诺。提取有原文证据的伏笔、悬念、人物转变、目标或关键场面，宁可返回空候选，不把每个描写都当成必须兑现的承诺。调用 submit_commitment_extraction，chapterId 照抄 target；payload 包含 chapterId、sourceRevisionId、examinedRanges:[{start,end}]、candidates。范围按原文 Unicode 字符位置排序，不重叠，只声明实际阅读范围。${handoff.task.target?.runId ? "连续创作任务必须实际阅读整章，再声明从0到正文字符数的连续覆盖范围。上下文截断或无法处理完整正文时应报告失败，不能虚构已读范围以通过任务门禁。" : ""}每个 candidate 包含批次内唯一 id、kind(foreshadowing/suspense/characterTransformation/goal/setPiece)、summary、relatedEntityIds、plotThreadId、characterArcId、setupEvidenceRefs:[{objectId,revisionId,quote,start,end}]、fulfillmentCriteria、targetWindow:{kind:"open"}、status:"open"、fulfillmentEvidenceRefs:[]、decisionReason:null。摘录必须精确匹配绑定原文并位于 examinedRanges；无法解析的关联留空。最多16项，整个报告不超过40KB。首次提交的 expectedCandidateRevisionId 从 inputManifest 读取；重试复用 idempotencyKey，后续提交使用返回 revisionId。最后 complete_task。这只是提案报告，不能声称独立承诺已建立、作者已确认、文学承诺已兑现或本章绝无伏笔。`;
  }
  if (handoff.task.operation === "modelKnowledge") {
    return `Novel Runtime 知识建模任务绑定：\n${JSON.stringify(handoff, null, 2)}\n\n必须先调用 novel_workspace.read_context，原样传入 taskId、projectId、threadId、expectedRevision，purpose=critic，budget=24，byteBudget=60000。返回的是 Context Pack；只读取 relevantText 中 sourceRevisionId 等于 operationContext.modelingBatch.sourceRevisionId 的 content.body，不将其他版本或计划当成已发生事实。operationContext.entityCatalog 仅为当前候选实体身份目录；批次与来源修订由 Runtime 根据 batchId 和冻结任务关联，提交的 entity/assertion 不自行添加 modelingBatchId、sourceRevisionId 或 assertion id；evidence 必须是仅含 quote、start、end 的单个对象，不是数组，也不添加来源字段，ambiguousNames 必须保持歧义，目录缺项也不等于实体不存在。先提取有原文证据的 entities（人物、组织、地点、物品、命题、事件），每个实体提供稳定 id、kind、displayName、aliases 和 evidence；再提取 assertion、event、relation 或 epistemic 候选。relation 的 subjectId、targetId、propositionId 必须引用本批次实体。epistemic 可引用本批次命题；若本章原文明确写出人物获知了前章命题，可用 propositionSourceBatchId 指向 entityCatalog 中该命题来源 claim 的 modelingBatchId，并将 propositionId 与 subjectId 设为该命题 ID，此时本批次不要重复声明命题。Runtime 会核验引用来自当前有效的前文章节；不可仅凭同名或同 ID 猜测。每条 evidence.quote 必须与本章草稿原文完全一致，start/end 使用 Unicode 字符位置，end 不包含结束字符。认知候选必须给出 epistemicOwnerId、epistemicState 和 propositionId；storyTime 与 epistemicTime 不可混用，无法确定时间时省略。没有可靠证据时提交空数组，不编造事实。调用 publish_knowledge_batch，batchId 必须等于 task.target.batchId，entities 和 assertions 均为候选数组；最后调用 complete_task。候选知识不等于作者确认的 Canon。`;
  }
  return `Novel Runtime 任务绑定：\n${JSON.stringify(handoff, null, 2)}\n\n必须先调用 novel_workspace.read_context，并原样传入 taskId、projectId、threadId、expectedRevision，purpose 使用绑定中的 task.purpose，budget=24，byteBudget=60000；Writer 还必须从 task.target 原样传入 readingChapter、sceneOrder、povCharacterId 和 storyTime（后两项未指定时省略）。返回的 Context Pack 中，stateFacts 是已确认事实，relevantText 含正文与 sourceRevisionId，plannedEffects 是规划而非已发生事实，operationContext.workingKnowledge 为候选工作知识；candidateCommitmentProposals 是尚未建立的批次提案，不是 Canon 或作者确认的承诺；规划可以考虑它们，但不得宣称已经兑现或强制采用，必须保留 reportRevisionId + proposalId + sourceRevisionId 的来源身份。作者现有指令和已建立承诺优先于提案。operationContext.commitmentProposalCoverage 的缺口不等于不存在承诺。先阅读 coverage.missingScope 与 sourceWatermarks，不能把漏扫范围当作没有事实。若来源或目标过期，应停止并请求新任务，不得自行切换输入版本。第一次写入使用 inputManifest.expectedCandidateRevisionId 做候选 CAS，后续检查点使用上一次保存返回的 revisionId。每次逻辑写入生成一个 idempotencyKey，重试同一次写入时复用该键。写作任务只能保存到 target 指定章节和范围，最终检查点使用 completeness=chapterComplete。所有产物只能通过 novel_workspace MCP 保存为候选版本；结束前调用 complete_task，由 Runtime 验证已保存产物；不得声称候选已被作者采用。`;
}
