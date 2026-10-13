// @ts-check
export function applyAgentConfiguration({ adapter, configuration, persist }) {
  adapter.assertConfigurable();
  persist();
  adapter.configure(configuration);
  return adapter.status();
}
