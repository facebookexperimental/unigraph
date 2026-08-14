// Values that need escaping in one or more target languages.
enum TrickyConsts: string as string {
  // Double quotes and backslashes.
  QUOTED = "say \"hi\" \\ bye";
  // Hack interpolates `$name` inside double-quoted strings.
  DOLLAR = "{\$notAVariable}";
  APOSTROPHE = "it's";
  NEWLINE = "line1\nline2";
}
