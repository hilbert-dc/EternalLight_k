import type { ConversationExecution } from './useConversationExecution';
import type { ExecutionSession } from './useExecutionSession';

// Only lifecycle adapters inside conversation/ can access mutable execution state.
const sessions = new WeakMap<ConversationExecution, ExecutionSession>();
export function bindExecution(controller: ConversationExecution, session: ExecutionSession) { sessions.set(controller, session); }
export function executionSession(controller: ConversationExecution): ExecutionSession {
  const session = sessions.get(controller);
  if (!session) throw new Error('Execution controller is not mounted');
  return session;
}
