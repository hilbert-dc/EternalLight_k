import { useCallback, useRef, useState } from "react";

export interface SupplementalEditor {
  dirty: boolean;
  busy: boolean;
  save: () => Promise<boolean>;
  discard: () => boolean;
}
export type RegisterSupplementalEditor = (editor: SupplementalEditor | null) => void;

export function useSupplementalEditor() {
  const current = useRef<SupplementalEditor | null>(null);
  const [state, setState] = useState({ active: false, dirty: false, busy: false });
  const register = useCallback<RegisterSupplementalEditor>((editor) => {
    current.current = editor;
    const next = { active: editor !== null, dirty: editor?.dirty ?? false, busy: editor?.busy ?? false };
    setState((before) => before.active === next.active && before.dirty === next.dirty && before.busy === next.busy ? before : next);
  }, []);
  return { ...state, current, register };
}
