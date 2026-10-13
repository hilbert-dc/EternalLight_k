type Target = Record<string, unknown>;
const object = (value: unknown): Target | null => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Target : null;
const positive = (value: unknown): number | null => Number.isSafeInteger(value) && Number(value) > 0 ? Number(value) : null;
const identifier = (value: unknown): value is string => typeof value === "string" && value.trim().length > 0
  && new TextEncoder().encode(value).length <= 160 && !/[\p{Cc}]/u.test(value);

const operations: Record<string, { name: string; purpose: string | null }> = {
  advisory: { name: "创作讨论", purpose: null },
  generateBrief: { name: "生成简报", purpose: "简报" },
  generateOutline: { name: "生成大纲", purpose: "大纲" },
  generatePropertyRules: { name: "生成属性规则提案", purpose: "属性规则提案" },
  generatePlan: { name: "生成近章规划", purpose: "近章规划" },
  generateChapter: { name: "生成章节", purpose: "写作" },
  writeNextChapter: { name: "续写章节", purpose: "写作" },
  modelKnowledge: { name: "分析章节设定", purpose: "设定分析" },
  assessRevisionOutcome: { name: "独立检查章节", purpose: "章节检查" },
  reviewQuality: { name: "独立读者审阅", purpose: "读者审阅" },
  assessChapterStructure: { name: "检查删章后的故事衔接", purpose: "删章衔接检查" },
  extractCommitments: { name: "提取伏笔候选", purpose: "伏笔提取" },
  reviseCommitment: { name: "修订伏笔候选", purpose: "伏笔修订" },
  reviewPlanningSeat: { name: "规划检查", purpose: "规划检查" },
  assessSceneOutcome: { name: "检查场景结果", purpose: "场景检查" },
};

export function taskTargetChapter(target: Target): number | null {
  return positive(target.chapterNumber) ?? positive(object(target.conversationFocus)?.chapterNumber);
}

function description(operation: string | null, target: Target) {
  const entry = operation && Object.hasOwn(operations, operation) ? operations[operation] : { name: "处理创作内容", purpose: null };
  if (operation === "reviewPlanningSeat") {
    const artifact = target.artifactKind === "outline" ? "大纲" : target.artifactKind === "plan" ? "近章规划" : null;
    const role = target.role === "structure" ? "结构与因果" : target.role === "character" ? "人物与主题" : target.role === "experience" ? "阅读体验" : null;
    if (role) return { name: `${artifact ? `${artifact} · ` : ""}${role}检查`, purpose: `${artifact ?? ""}${role}检查` };
  }
  if (operation === "generateChapter") {
    const scope = object(target.scope);
    const groups = scope?.groupIds;
    if (scope?.type === "selectedScenes" && identifier(scope.sourceRevisionId) && Array.isArray(groups)
      && groups.length > 0 && groups.length <= 16 && groups.every(identifier) && new Set(groups).size === groups.length) {
      return Array.isArray(scope.bodyRanges) && scope.bodyRanges.length > 0
        ? { name: "修改选定文字", purpose: "局部文字修改" }
        : { name: "修改选定场景", purpose: "选定场景修改" };
    }
    const end = positive(scope?.endSceneOrder);
    if (scope?.type === "throughScene" && end && end <= 100 && target.sceneOrder === end) {
      return { name: `写作至第 ${end} 个场景`, purpose: `前 ${end} 个场景写作` };
    }
    if (scope?.type === "wholeChapter") return { name: "整章写作", purpose: "整章写作" };
  }
  return entry;
}

/** One vocabulary for steps, proposals, confirmation and recovery; no result claims. */
export function taskOperationName(operation: string | null, target: Target = {}): string {
  const from = operation === "generatePlan" ? positive(target.planFromChapter) : null;
  if (from) return `从第 ${from} 章起生成近章规划`;
  const number = taskTargetChapter(target);
  return `${number ? `第 ${number} 章 · ` : ""}${description(operation, target).name}`;
}

export function taskOperationPurpose(operation: string, target: Target): string | null {
  const from = operation === "generatePlan" ? positive(target.planFromChapter) : null;
  if (from) return `第 ${from} 章起的近章规划`;
  const purpose = description(operation, target).purpose;
  const number = taskTargetChapter(target);
  return purpose ? `${number ? `第 ${number} 章` : ""}${purpose}` : null;
}

export function compactTaskName(fullLabel: string): { label: string; fullLabel: string } {
  const characters = Array.from(fullLabel);
  return { label: characters.length > 60 ? `${characters.slice(0, 60).join("")}…` : fullLabel, fullLabel };
}
