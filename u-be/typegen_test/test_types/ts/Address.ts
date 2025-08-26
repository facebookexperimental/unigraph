/** Simple address struct for testing */
export interface Address {
  /** Street address */
  street: string;
  city: string;
  zip_code: number;
  coordinates: [number, number, number];
  typegen_as: number;
  string_list: string[];
  maybe_flag?: boolean | undefined;
  tags: string[];
}