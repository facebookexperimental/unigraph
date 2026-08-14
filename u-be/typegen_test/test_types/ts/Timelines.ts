/** Well-known timeline identifiers. */
export const Timelines = {
  /** The main timeline. */
  MY_TIMELINE: "timeline-123",
  OTHER_TIMELINE: "timeline-456",
} as const;

export type Timelines = (typeof Timelines)[keyof typeof Timelines];
