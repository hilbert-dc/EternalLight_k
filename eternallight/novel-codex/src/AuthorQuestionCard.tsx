import { useRef, useState } from "react";
import "./AuthorQuestionCard.css";
import { authorAnswerCapacityError } from "../shared/author-answer-budget.mjs";

import type { UserInputQuestion } from "../shared/author-questions.mjs";

export type UserInputResponse = { answers: Record<string, string | string[]> };
export type { UserInputQuestion } from "../shared/author-questions.mjs";
export type ResolveUserInput = (response: UserInputResponse) => Promise<boolean> | boolean | void;
type Props = { requestIdentity: string; questions: UserInputQuestion[]; onResolve: ResolveUserInput };

/** Each real request owns its answers, including when consecutive requests reuse question IDs. */
export function AuthorQuestionCard({ requestIdentity, ...props }: Props) {
  return <QuestionAnswers key={requestIdentity} {...props} />;
}

function QuestionAnswers({ questions, onResolve }: Omit<Props, "requestIdentity">) {
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [submitting, setSubmitting] = useState(false);
  const submittingRef = useRef(false);
  const inputs = useRef(new Map<string, HTMLTextAreaElement>());
  const [error, setError] = useState<string | null>(null);
  const answerFor = (id: string) => Object.hasOwn(answers, id) ? answers[id] : "";
  const setAnswer = (id: string, value: string) => setAnswers(current => ({ ...current, [id]: value }));
  const canSubmit = questions.length > 0 && questions.every(question => Boolean(answerFor(question.id).trim()));

  async function submit() {
    if (!canSubmit || submittingRef.current) return;
    const response = { answers: Object.fromEntries(questions.map(question => [question.id, answerFor(question.id)])) };
    const capacityError = authorAnswerCapacityError(JSON.stringify(response));
    if (capacityError) { setError(capacityError); return; }
    submittingRef.current = true;
    setSubmitting(true);
    setError(null);
    try {
      const accepted = await onResolve(response);
      if (accepted === false) setError("回答未能发送，请重试。");
      else setAnswers({});
    } catch (reason) { setError(reason instanceof Error ? reason.message : "回答未能发送，请重试。"); }
    finally { submittingRef.current = false; setSubmitting(false); }
  }

  return <div className="approval-card user-input-card">
    {questions.length === 0 ? <div className="user-input-empty">当前请求没有可展示的问题。</div>
      : <div className="approval-questions">{questions.map((question, index) => <fieldset className="approval-question" key={question.id} disabled={submitting}>
        <legend>{question.header ?? `问题 ${index + 1}`}</legend>
        <p>{question.question}</p>
        <textarea className="approval-input" ref={node => { if (node) inputs.current.set(question.id, node); else inputs.current.delete(question.id); }}
          value={answerFor(question.id)} rows={2} aria-label={question.question}
          onChange={event => setAnswer(question.id, event.target.value)}
          onKeyDown={event => {
            if (event.key === "Enter" && (event.ctrlKey || event.metaKey) && !event.nativeEvent.isComposing) {
              event.preventDefault(); void submit();
            }
          }} placeholder={question.options.length ? "直接回答，也可以选一个建议再补充" : "输入你的回答"} />
        {question.options.length > 0 && <div className="approval-question-options author-question-options">{question.options.map((option, optionIndex) => <button
          type="button" className="approval-option author-question-option" key={`${optionIndex}:${option.label}`}
          aria-label={`使用建议：${option.label}`} title="填入回答后可以继续补充"
          onClick={() => { setAnswer(question.id, option.label); inputs.current.get(question.id)?.focus(); }}>
          <span><strong>{option.label}</strong>{option.description && <small>{option.description}</small>}</span>
        </button>)}</div>}
      </fieldset>)}</div>}
    {error && <div className="user-input-error" role="alert">{error}</div>}
    <div className="approval-card-actions"><button type="button" className="primary-button" disabled={!canSubmit || submitting}
      aria-keyshortcuts="Control+Enter Meta+Enter" title="Ctrl+Enter 或 ⌘+Enter 提交回答" onClick={() => void submit()}>
      {submitting ? "发送中…" : "提交回答"}
    </button></div>
  </div>;
}
