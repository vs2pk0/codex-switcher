export function becameHiddenAccount(
  previous: boolean | null | undefined,
  next: boolean | null | undefined,
): boolean {
  return !Boolean(previous) && Boolean(next);
}

export function shouldCleanupHiddenAccount(input: {
  previousHidden?: boolean | null;
  nextHidden?: boolean | null;
  previousPending?: boolean | null;
  nextPending?: boolean | null;
}): boolean {
  return (
    Boolean(input.nextHidden) &&
    (becameHiddenAccount(input.previousHidden, input.nextHidden) ||
      Boolean(input.previousPending) ||
      Boolean(input.nextPending))
  );
}
