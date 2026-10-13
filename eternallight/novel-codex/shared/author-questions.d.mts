export type UserInputQuestion = {
  id: string;
  header: string | null;
  question: string;
  options: { label: string; description: string | null }[];
  isOther: boolean;
};
export const MAX_AUTHOR_QUESTION_OPTIONS: number;
export function userInputQuestions(params: unknown): UserInputQuestion[];
