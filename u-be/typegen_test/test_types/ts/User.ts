/** Test struct with optional fields */
export interface User {
  id: number;
  email: string;
  profile?: string | undefined;
  verified: boolean;
  tags: { [key: string]: string };
  metadata: { [key: string]: boolean };
}
