import { add, calculate, Square } from './model';

export const square = new Square(4);
export const total = add(4, 5);
export const result = calculate() + square.area();
