// Independent expectation from the cloned console's default policy.
export function consolePartSize(size) {
  const MiB = 1024 * 1024;
  return size > 1024 * MiB ? 50 * MiB : size < 16 * MiB ? Math.max(size, 1) : size < 256 * MiB ? 4 * MiB : 8 * MiB;
}
