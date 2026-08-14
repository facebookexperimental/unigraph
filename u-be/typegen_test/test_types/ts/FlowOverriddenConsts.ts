/**
 * Const group whose Flow output is overridden to a plain type alias.
 * 
 * An override emits no runtime values, so this one stays a `.js.flow`
 * declaration file rather than becoming a `.js` module.
 */
export const FlowOverriddenConsts = {
  SOME_VALUE: "value",
} as const;

export type FlowOverriddenConsts = (typeof FlowOverriddenConsts)[keyof typeof FlowOverriddenConsts];
