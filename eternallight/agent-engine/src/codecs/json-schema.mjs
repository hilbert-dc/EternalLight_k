import { nativeCall } from "../native.mjs";
export const assertSupportedToolSchema = (schema) => nativeCall("schema.check", { schema });
export const toolSchemaForProtocolConversion = (schema) => nativeCall("schema.convert", { schema });
export const assertMatchesToolSchema = (value, schema, label = "tool arguments") => nativeCall("schema.validate", { value, schema, label });
