// A protocol projection shared by Host and every creative surface. Never shorten prose or IDs.
export const MAX_AUTHOR_QUESTION_OPTIONS = 6;
const optionalText = value => typeof value === "string" && value.trim() ? value : null;
export function userInputQuestions(params) {
  if (!Array.isArray(params?.questions)) return [];
  return params.questions.slice(0, 3).flatMap(question => {
    if (!question || typeof question.id !== "string" || !question.id.trim()
      || question.id.length > 240 || /[\r\n]/.test(question.id) || typeof question.question !== "string") return [];
    const options = Array.isArray(question.options) ? question.options.slice(0, MAX_AUTHOR_QUESTION_OPTIONS)
      .filter(option => typeof option?.label === "string" && option.label.trim())
      .map(option => ({ label: option.label, description: optionalText(option.description) })) : [];
    return [{ id: question.id, header: optionalText(question.header), question: question.question,
      options, isOther: question.isOther === true }];
  });
}
