export function getChartNavigationIndex(
  key: string,
  currentIndex: number,
  count: number,
): number | null {
  if (count <= 0 || currentIndex < 0 || currentIndex >= count) return null;

  switch (key) {
    case 'ArrowLeft':
    case 'ArrowUp':
      return Math.max(0, currentIndex - 1);
    case 'ArrowRight':
    case 'ArrowDown':
      return Math.min(count - 1, currentIndex + 1);
    case 'Home':
      return 0;
    case 'End':
      return count - 1;
    default:
      return null;
  }
}
