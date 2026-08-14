/** Const group that opts out of Flow and overrides the Hack output. */
export const PartialConsts = {
  ONLY_SOME_LANGUAGES: "value",
} as const;

export type PartialConsts = (typeof PartialConsts)[keyof typeof PartialConsts];
