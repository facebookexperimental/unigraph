import type { Decision } from './Decision.ts';

export interface ForceDynamic {
  from_node?: string | null;
  match_properties: { [key: string]: string };
  branch?: string | null;
  decision: Decision;
}