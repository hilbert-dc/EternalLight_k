import { useGenerationActions } from '../useGenerationActions';
import { useExecutionRecovery } from '../useExecutionRecovery';
import { useConversationTaskActions } from '../useConversationTaskActions';
import type { ExecutionSession } from './useExecutionSession';
import type { ConversationExecution } from './useConversationExecution';
import { executionSession } from './execution-internals';

type Ports<T> = Omit<T, keyof ExecutionSession>;
export function useConversationGeneration(controller: ConversationExecution, ports: Ports<Parameters<typeof useGenerationActions>[0]>) {
  return useGenerationActions({ ...executionSession(controller), ...ports });
}
export function useConversationRecovery(controller: ConversationExecution, ports: Ports<Parameters<typeof useExecutionRecovery>[0]>) {
  return useExecutionRecovery({ ...executionSession(controller), ...ports });
}
export function useConversationTaskCommands(controller: ConversationExecution, ports: Ports<Parameters<typeof useConversationTaskActions>[0]>) {
  return useConversationTaskActions({ ...executionSession(controller), ...ports });
}
