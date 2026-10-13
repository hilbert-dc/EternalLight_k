import { useEffect, useRef, useState } from "react";
import type { NovelCodexApi, ViewId } from "./types";

export function usePlanningSubmission(options: {
  api: NovelCodexApi | undefined;
  projectId: string;
  view: ViewId | "account";
  eligible: boolean;
  composer: string;
  sourceIdentity: string | null;
  canSendAfterSave: () => boolean;
}) {
  const scope = useRef({ api: options.api, projectId: options.projectId, view: options.view, eligible: options.eligible });
  if (scope.current.api !== options.api || scope.current.projectId !== options.projectId
    || scope.current.view !== options.view || scope.current.eligible !== options.eligible) {
    scope.current = { api: options.api, projectId: options.projectId, view: options.view, eligible: options.eligible };
  }
  const owner = useRef({ ...options });
  const previous = owner.current;
  if (previous.api !== options.api || previous.projectId !== options.projectId
    || previous.view !== options.view || previous.eligible !== options.eligible
    || previous.composer !== options.composer || previous.sourceIdentity !== options.sourceIdentity) {
    owner.current = { ...options };
  } else {
    owner.current.canSendAfterSave = options.canSendAfterSave;
  }
  const renderedOwner = owner.current;
  const flight = useRef<{ owner: typeof owner.current; scope: typeof scope.current } | null>(null);
  const mounted = useRef(true);
  const [, render] = useState<object>({});
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; flight.current = null; };
  }, []);

  function isPending() {
    return mounted.current && flight.current?.scope === scope.current;
  }

  function invalidate() {
    owner.current = { ...owner.current };
    if (mounted.current) render({});
  }

  function finish() {
    if (flight.current?.owner !== renderedOwner) return;
    flight.current = null;
    if (mounted.current) render({});
  }

  async function prepare(save: () => Promise<boolean>) {
    const captured = owner.current;
    if (captured !== renderedOwner || !mounted.current || !captured.api
      || !captured.projectId || !captured.eligible || isPending()) return false;
    const request = { owner: captured, scope: scope.current };
    flight.current = request;
    render({});
    const current = () => mounted.current && owner.current === captured && flight.current === request;
    const release = () => {
      if (flight.current === request) {
        flight.current = null;
        if (mounted.current) render({});
      }
    };
    try {
      if (!await save() || !current() || !owner.current.canSendAfterSave()) {
        release();
        return false;
      }
      // App binds its prompt scope and calls finish synchronously, without another await.
      return true;
    } catch (reason) {
      release();
      if (owner.current === captured && mounted.current) throw reason;
      return false;
    }
  }

  return { pending: isPending(), isPending, prepare, finish, invalidate };
}
