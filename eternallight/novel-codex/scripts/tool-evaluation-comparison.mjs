// Experiment obligations, not production permissions or literary judgments.
const emptyQuery = '44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a';
export const readingPolicies = ['task', 'completeOriginalAndFinal'];
export function sumLedgerUsage(cases) {
  const fields = ['requests', 'tokens', 'unknownTokens'];
  if (!cases.length || cases.some(c => fields.some(k => !Number.isSafeInteger(c.usage?.[k]) || c.usage[k] < 0))) return null;
  const total = Object.fromEntries(fields.map(k => [k, cases.reduce((n, c) => n + c.usage[k], 0)]));
  return fields.every(k => Number.isSafeInteger(total[k])) ? total : null;
}
export function readingInstructions(policy) {
  return policy === 'completeOriginalAndFinal'
    ? '同条件阅读验收：保存前必须用 content 视图完整读取 read_context 中 /relevantText 来源，沿 nextRead 读至结束；locate、目录、摘要不能替代完整阅读。最后一次保存后，必须沿 savedSource.read 及 nextRead 完整读取该版本，再调用 complete_task。若无法完成，明确报告未完成，不宣称已完整核验。'
    : '先读取原文，保持原授权范围。';
}
function completeSource(tools, source) {
  if (!source) return false;
  // Follow an actual delivered cursor chain, retaining source/hash/total identity.
  for (let start = 0; start < tools.length; start++) {
    const first = tools[start];
    const r = first.read;
    if (first.error || first.status !== 'completed' || !r || r.source !== source
      || r.returnedSource !== source || r.view !== 'content' || r.query !== emptyQuery
      || r.cursor !== null || !r.contentHash || r.coverage?.fromEntry !== 0) continue;
    const total = r.coverage.totalEntries;
    if (!Number.isSafeInteger(total) || total <= 0) continue;
    let offset = 0, cursor = null;
    for (let i = start; i < tools.length; i++) {
      const t = tools[i], page = t.read;
      if (t.error || t.status !== 'completed' || !page || page.source !== source
        || page.returnedSource !== source || page.view !== 'content' || page.query !== emptyQuery
        || page.contentHash !== r.contentHash || page.cursor !== cursor
        || page.coverage?.fromEntry !== offset || page.coverage.totalEntries !== total) continue;
      const end = page.coverage.toEntry;
      if (!Number.isSafeInteger(end) || end <= offset || end > total) continue;
      offset = end; cursor = page.nextCursor;
      if (offset === total) return cursor === null;
      if (!cursor) break;
    }
  }
  return false;
}
export function assessReading(tools, policy) {
  if (policy === 'task') return { policy, honored: null, originalCompleteBeforeSave: null, finalCompleteBeforeCompletion: null };
  if (policy !== 'completeOriginalAndFinal') throw Error('Unknown reading policy');
  const saves = tools.map((t, i) => ({ t, i })).filter(({ t }) => t.save?.saved && !t.error && t.status === 'completed');
  const first = saves[0], last = saves.at(-1);
  const source = tools.slice(0, first?.i ?? 0).find(t => !t.error && t.status === 'completed' && t.context?.originalSource)?.context.originalSource;
  const completion = tools.findIndex((t, i) => i > (last?.i ?? tools.length) && t.name === 'complete_task' && !t.error && t.status === 'completed');
  const originalCompleteBeforeSave = !!first && completeSource(tools.slice(0, first.i), source);
  const finalCompleteBeforeCompletion = !!last && completion >= 0
    && completeSource(tools.slice(last.i + 1, completion), last.t.save.source);
  return { policy, honored: originalCompleteBeforeSave && finalCompleteBeforeCompletion,
    originalCompleteBeforeSave, finalCompleteBeforeCompletion };
}
export function summarizeComparablePairs(cases, { repeats, caseSelection, readingPolicy }, buildStable) {
  const pairs = [];
  for (let repeat = 0; repeat < repeats; repeat++) for (const size of ['short', 'long']) {
    if (caseSelection && !caseSelection.some(c => c.size === size)) continue;
    const rows = cases.filter(c => c.repeat === repeat && c.size === size);
    const full = rows.filter(c => c.mode === 'full'), patch = rows.filter(c => c.mode === 'patch');
    const reasons = [];
    if (full.length !== 1 || patch.length !== 1) reasons.push('missing_or_duplicate_pair');
    if (readingPolicy !== 'completeOriginalAndFinal') reasons.push('uncontrolled_reading');
    if (buildStable !== true) reasons.push('build_identity_not_stable');
    if (rows.some(c => c.status !== 'passed' || c.reading?.honored !== true)) reasons.push('case_failed_or_reading_incomplete');
    if (rows.some(c => !c.usage || c.usage.unknownTokens !== 0)) reasons.push('unknown_usage');
    if (rows.length && (!rows[0].fixtureDigest || rows.some(c => c.fixtureDigest !== rows[0].fixtureDigest))) reasons.push('fixture_mismatch');
    if (rows.length && (!rows[0].admittedCatalogDigest || rows.some(c => c.admittedCatalogDigest !== rows[0].admittedCatalogDigest))) reasons.push('admitted_catalog_mismatch');
    const eligible = reasons.length === 0;
    pairs.push({ repeat, size, eligible, reasons, fullTokens: full[0]?.usage?.tokens ?? null,
      patchTokens: patch[0]?.usage?.tokens ?? null,
      patchToFullTokenRatio: eligible && full[0].usage.tokens > 0 ? patch[0].usage.tokens / full[0].usage.tokens : null });
  }
  return { pairs, eligiblePairs: pairs.filter(p => p.eligible).length,
    interpretation: 'Same-run full versus patch comparison under verified reading obligations; not an A/B test of earlier code or literary quality.' };
}
export function summarizePayloadCost(record) {
  const buckets = {};
  const originals = new Set(record.tools.map(t => t.context?.originalSource).filter(Boolean));
  const candidates = new Set(record.tools.map(t => t.save?.source).filter(Boolean));
  for (const t of record.tools) {
    const category = t.name === 'read_source_content' ? (['index', 'workspace'].includes(t.read?.view) ? 'sourceDirectory'
      : originals.has(t.read?.source) ? 'originalSourceReading'
      : candidates.has(t.read?.source) ? 'savedCandidateReading' : 'otherSourceReading')
      : ['read_tool_guide', 'discover_creative_capabilities'].includes(t.name) ? 'toolGuidance'
      : t.name === 'read_context' ? 'taskContext'
      : t.name === 'save_draft_checkpoint' ? 'saveReceipt' : 'completionOrOther';
    const b = buckets[category] ??= { calls: 0, resultBytes: 0, unknownSizeCalls: 0 };
    b.calls++;
    if (Number.isSafeInteger(t.resultBytes) && t.resultBytes >= 0) b.resultBytes += t.resultBytes;
    else b.unknownSizeCalls++;
  }
  return { toolResults: buckets, requestPayloads: record.requests.map(r => ({ instructionsBytes: r.instructionsBytes, toolsBytes: r.toolsBytes, inputBytes: r.inputBytes })),
    interpretation: 'Serialized bytes, not tokens or prices. Input includes prior conversation; source reading and repetition are not automatically waste. Exact-edit cases do not isolate literary reasoning.' };
}
