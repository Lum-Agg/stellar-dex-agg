export type TokenLike = {
  id: string;
};

export function resolveTokenSelection<T extends TokenLike>({
  current,
  other,
  next,
}: {
  current: T;
  other: T;
  next: T;
}): {
  current: T;
  other: T;
  swapped: boolean;
} {
  if (next.id === other.id) {
    return {
      current: other,
      other: current,
      swapped: true,
    };
  }

  return {
    current: next,
    other,
    swapped: false,
  };
}
