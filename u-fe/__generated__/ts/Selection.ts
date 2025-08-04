import type { SelectionType } from './SelectionType.ts';
import type { TsVec2 } from './TsVec2.ts';

export interface Selection {
  selection_from_point: TsVec2;
  selection_to_point: TsVec2;
  selection_type: SelectionType;
}