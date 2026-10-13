// @ts-check
import { createHash } from 'node:crypto';
import { EXECUTION_POWER_POLICY } from './execution-domains.mjs';
// Only call with a SessionPlan that passed the current-evidence gate.
// Qualification timestamps and random IDs must not refresh recovery attempts.
export function executionConditionDigest(plan, qualificationContextDigest) {
  if (!plan || !Array.isArray(plan.routes) || !plan.routes.length || !qualificationContextDigest) throw new Error('缺少已验证的执行条件');
  const routes = plan.routes.map(route => ({ logicalModel: route.logicalModel, model: route.model, provider: route.provider,
    protocol: route.protocol, endpoint: route.endpoint, environmentDigest: route.environmentDigest,
    enabledExtensions: [...(route.enabledExtensions ?? [])].sort(), inputBudget: route.inputBudget, outputBudget: route.outputBudget }))
    .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  return createHash('sha256').update(JSON.stringify([qualificationContextDigest, plan.rootRoute, plan.engineContractHash, plan.policyDigest, routes, { executionPowerPolicy: EXECUTION_POWER_POLICY }])).digest('hex');
}
