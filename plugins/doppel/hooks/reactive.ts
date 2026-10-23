/**
 * The few stream operators the module uses, in the shape of rxjs without the
 * dependency: a hooks module can only import files of its own plugin.
 */

export type Observer<T> = (value: T) => void

export type Unsubscribe = () => void

export interface Observable<T> {
  subscribe(observer: Observer<T>): Unsubscribe
}

export class Subject<T> implements Observable<T> {
  private readonly observers = new Set<Observer<T>>()

  next(value: T): void {
    for (const observer of [...this.observers]) {
      observer(value)
    }
  }

  subscribe(observer: Observer<T>): Unsubscribe {
    this.observers.add(observer)

    return () => {
      this.observers.delete(observer)
    }
  }
}

export function map<T, R>(source: Observable<T>, project: (value: T) => R): Observable<R> {
  return { subscribe: observer => source.subscribe(value => observer(project(value))) }
}

export function filter<T, S extends T>(source: Observable<T>, keep: (value: T) => value is S): Observable<S>
export function filter<T>(source: Observable<T>, keep: (value: T) => boolean): Observable<T>
export function filter<T>(source: Observable<T>, keep: (value: T) => boolean): Observable<T> {
  return {
    subscribe: observer =>
      source.subscribe(value => {
        if (keep(value)) {
          observer(value)
        }
      }),
  }
}

/** Emits every intermediate accumulation, starting from `seed` for each subscriber. */
export function scan<T, A>(source: Observable<T>, step: (acc: A, value: T) => A, seed: A): Observable<A> {
  return {
    subscribe: observer => {
      let acc = seed

      return source.subscribe(value => {
        acc = step(acc, value)
        observer(acc)
      })
    },
  }
}

export function merge<T>(...sources: Observable<T>[]): Observable<T> {
  return {
    subscribe: observer => {
      const unsubscribes = sources.map(source => source.subscribe(observer))

      return () => {
        for (const unsubscribe of unsubscribes) {
          unsubscribe()
        }
      }
    },
  }
}
