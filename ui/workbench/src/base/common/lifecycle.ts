export interface IDisposable {
  dispose(): void;
}

export function toDisposable(fn: () => void): IDisposable {
  return { dispose: fn };
}

export class DisposableStore implements IDisposable {
  private readonly disposables = new Set<IDisposable>();
  private isDisposed = false;

  add<T extends IDisposable>(disposable: T): T {
    if (this.isDisposed) {
      disposable.dispose();
      return disposable;
    }
    this.disposables.add(disposable);
    return disposable;
  }

  clear(): void {
    for (const disposable of this.disposables) {
      disposable.dispose();
    }
    this.disposables.clear();
  }

  dispose(): void {
    if (this.isDisposed) {
      return;
    }
    this.isDisposed = true;
    this.clear();
  }
}

export abstract class Disposable implements IDisposable {
  private readonly store = new DisposableStore();

  protected _register<T extends IDisposable>(disposable: T): T {
    return this.store.add(disposable);
  }

  dispose(): void {
    this.store.dispose();
  }
}
