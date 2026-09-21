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

export function resolveUrlTokenPair<T extends TokenLike>({
  currentIn,
  currentOut,
  requestedIn,
  requestedOut,
  available,
}: {
  currentIn: T;
  currentOut: T;
  requestedIn?: T;
  requestedOut?: T;
  available: T[];
}): { tokenIn: T; tokenOut: T; sameTokenRejected: boolean } {
  let tokenIn = requestedIn ?? currentIn;
  let tokenOut = requestedOut ?? currentOut;

  if (requestedIn && !requestedOut && tokenIn.id === tokenOut.id) {
    tokenOut = available.find((token) => token.id !== tokenIn.id) ?? currentOut;
  }
  if (requestedOut && !requestedIn && tokenIn.id === tokenOut.id) {
    tokenIn = available.find((token) => token.id !== tokenOut.id) ?? currentIn;
  }
  if (tokenIn.id === tokenOut.id) {
    return { tokenIn: currentIn, tokenOut: currentOut, sameTokenRejected: true };
  }

  return { tokenIn, tokenOut, sameTokenRejected: false };
}
