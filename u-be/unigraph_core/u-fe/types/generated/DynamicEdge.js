// @flow

export type DynamicEdge = {
  properties: { [string]: string };
  branches: { [string]: Array<NodeName> };
};
