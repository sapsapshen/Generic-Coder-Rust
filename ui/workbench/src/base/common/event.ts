import { Disposable, IDisposable, toDisposable } from './lifecycle';

export type Listener<T> = (event: T) => void;
export type Event<T> = (listener: Listener<T>) => IDisposable;

export class Emitter<T> extends Disposable {
  private readonly listeners = new Set<Listener<T>>();

  readonly event: Event<T> = (listener) => {
    this.listeners.add(listener);
    return toDisposable(() => {
      this.listeners.delete(listener);
    });
  };

  fire(event: T): void {
    for (const listener of [...this.listeners]) {
      listener(event);
    }
  }
}
