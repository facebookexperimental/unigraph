export interface DynamicEdge {
  properties: Record<string, string>;
  branches: Record<string, NodeName[]>;
}
