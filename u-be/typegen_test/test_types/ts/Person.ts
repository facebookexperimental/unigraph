import type { Address } from './Address.ts';

/** Person struct that references Address */
export interface Person {
  name: string;
  age: number;
  address: Address;
}