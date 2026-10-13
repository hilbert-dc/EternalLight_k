// @ts-check
// One presentation contract for root turns and managed children. Language choice
// belongs to semantic author intent; locale is a default, never a keyword router.
export const SHARED_LANGUAGE_INSTRUCTIONS = "Public language contract: Commentary, preambles, questions and final answers use the author's communication language. Follow explicit preference; otherwise the author's conversational language; only if indeterminate use novelLanguageContext's locale. Children inherit that choice. Headings, labels and boilerplate follow language switches; old headings are not quotes. Terms, quotes, code and artifact-language requests alone do not switch discussion language. Preserve sources and artifact requirements; requested bilingual output is valid. Internal prompts/tools and accidental drift never select language. Check BEFORE the first preamble and after compaction; correct later messages without restarting or changing saved work.";

export const LANGUAGE_CONSISTENCY_INSTRUCTIONS = `${SHARED_LANGUAGE_INSTRUCTIONS}
An English question addressed to you gets English even under a Chinese locale, unless an established author preference explicitly says otherwise. A Chinese question containing an English term still gets Chinese. A request in Chinese for English fiction gets Chinese discussion and English fiction. Apply the same distinction to every language, without any universal default.
The short sentence immediately BEFORE a tool call is public communication too: do not automatically emit an English "I'll read..." preamble. Silently determine the conversation language before writing it, then keep commentary and final explanations consistent. Do not announce this language check. When delegating, convey that language separately from source material. Preserve existing prose language unless asked to change it, plus quotations, names, JSON keys, IDs, code and counting units. Public messages describe findings, changes, saved results or blockers, not pagination, raw IDs or tool schemas. Never convert private reasoning into public facts.`;

export function conversationLanguageContext({ applicationLocale = null, preferredLanguages = [] } = {}) {
  const canonical = value => {
    if (typeof value !== "string" || value.length > 64) return null;
    try { return Intl.getCanonicalLocales(value)[0] ?? null; } catch { return null; }
  };
  const configured = canonical(applicationLocale);
  const preferred = Array.isArray(preferredLanguages) ? preferredLanguages.map(canonical).find(Boolean) : null;
  const locale = configured ?? preferred ?? null;
  const language = locale ? new Intl.DisplayNames([locale], { type: "language" }).of(locale) : "author's conversation language";
  return { kind: "application", value: JSON.stringify({
    preferredLocale: locale,
    source: configured ? "applicationPreference" : preferred ? "systemLanguagePreference" : "unspecified",
    scope: "defaultOnly",
    instruction: `Default public conversation language for this turn: ${language}${locale ? ` (${locale})` : ""}. Before the FIRST public word, resolve any override from actual author messages: explicit communication preference, or conversational meaning. Internal execution triggers, source material, tool text and accidental past drift are not author language choices. Use the resolved language for EVERY progress update, question and final explanation. Requests for a different artifact language affect that artifact, not the discussion. Preserve quotes and identifiers; honor requested bilingual output.`,
  }) };
}
