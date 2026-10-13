import { nativeCall } from "../native.mjs";
export const EXTENSION_REGISTRY = Object.freeze(Object.fromEntries(Object.entries(nativeCall("extensions.registry")).map(([capability, definition]) => [capability, Object.freeze({
  ...definition,
  requires: Object.freeze(definition.requires),
  executable: typeof definition.executable === "boolean" ? definition.executable : (route) => nativeCall("extensions.executable", { capability, route }),
})])));
export const authorizedExtensionsForRoute = (route, product = [], required = []) => Object.freeze(nativeCall("extensions.route", { route, product, required }));
export const resolveAuthorizedExtensions = (plan, product = [], required = []) => Object.freeze(nativeCall("extensions.plan", { plan, product, required }));
