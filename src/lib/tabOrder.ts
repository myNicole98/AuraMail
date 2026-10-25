export function insertionIndex(pointerX: number, centers: number[]): number {
  for (let index = 0; index < centers.length; index += 1) {
    if (pointerX < centers[index]) return index;
  }
  return centers.length;
}
