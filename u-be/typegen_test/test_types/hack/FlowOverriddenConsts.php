// Const group whose Flow output is overridden to a plain type alias.
// 
// An override emits no runtime values, so this one stays a `.js.flow`
// declaration file rather than becoming a `.js` module.
enum FlowOverriddenConsts: string as string {
  SOME_VALUE = "value";
}
