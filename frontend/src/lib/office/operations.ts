// Exporting and persisting a document share one queue. A user's Save waits for
// an in-flight local checkpoint instead of dropping the click. A failed task
// rejects its own caller without poisoning subsequent recovery/download work.
export class OfficeOperations {
  private tail: Promise<unknown> = Promise.resolve();
  run<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.tail.then(operation);
    this.tail = result.catch(() => undefined);
    return result;
  }
  async idle(): Promise<void> { await this.tail; }
}
