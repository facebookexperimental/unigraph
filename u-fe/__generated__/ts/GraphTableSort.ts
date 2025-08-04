import type { SortOrder } from './SortOrder.ts';

export interface GraphTableSort {
  column_id: string;
  order: SortOrder;
}