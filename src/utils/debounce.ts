/**
 * 防抖 / 节流 / 一次性工具。
 *
 * 搜索框输入、窗口 resize、侧栏宽度记忆都用得上。
 */

/** 带 `cancel` / `flush` 的防抖函数 */
export interface Debounced<T extends (...args: never[]) => unknown> {
  (...args: Parameters<T>): void;
  /** 取消尚未触发的调用 */
  cancel(): void;
  /** 立刻执行尚未触发的调用 */
  flush(): void;
  /** 是否还有待触发的调用 */
  pending(): boolean;
}

/**
 * 防抖：连续触发时只在停止 `wait` 毫秒后执行一次。
 *
 * @param fn   原始函数
 * @param wait 等待毫秒，默认 200
 */
export function debounce<T extends (...args: never[]) => unknown>(
  fn: T,
  wait = 200,
): Debounced<T> {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let lastArgs: Parameters<T> | null = null;

  const invoke = () => {
    timer = null;
    if (lastArgs) {
      const args = lastArgs;
      lastArgs = null;
      fn(...args);
    }
  };

  const debounced = ((...args: Parameters<T>) => {
    lastArgs = args;
    if (timer !== null) clearTimeout(timer);
    timer = setTimeout(invoke, wait);
  }) as Debounced<T>;

  debounced.cancel = () => {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    lastArgs = null;
  };

  debounced.flush = () => {
    if (timer !== null) {
      clearTimeout(timer);
      invoke();
    }
  };

  debounced.pending = () => timer !== null;

  return debounced;
}

/** 带 `cancel` 的节流函数 */
export interface Throttled<T extends (...args: never[]) => unknown> {
  (...args: Parameters<T>): void;
  cancel(): void;
}

/**
 * 节流：固定时间窗口内最多执行一次。
 *
 * @param fn    原始函数
 * @param wait  窗口毫秒，默认 100
 * @param opts.trailing 为 true 时窗口结束再补一次（默认 true）
 */
export function throttle<T extends (...args: never[]) => unknown>(
  fn: T,
  wait = 100,
  opts: { trailing?: boolean } = {},
): Throttled<T> {
  const trailing = opts.trailing !== false;
  let last = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let lastArgs: Parameters<T> | null = null;

  const run = (args: Parameters<T>) => {
    last = Date.now();
    fn(...args);
  };

  const throttled = ((...args: Parameters<T>) => {
    const now = Date.now();
    const remain = wait - (now - last);

    if (remain <= 0) {
      if (timer !== null) {
        clearTimeout(timer);
        timer = null;
      }
      run(args);
      return;
    }

    lastArgs = args;
    if (trailing && timer === null) {
      timer = setTimeout(() => {
        timer = null;
        if (lastArgs) {
          const a = lastArgs;
          lastArgs = null;
          run(a);
        }
      }, remain);
    }
  }) as Throttled<T>;

  throttled.cancel = () => {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    lastArgs = null;
  };

  return throttled;
}

/**
 * 生成一个短随机 ID（无外部依赖）。
 *
 * @param prefix 前缀，最终 id 形如 `toast_lz3k9f`
 */
export function shortId(prefix = ""): string {
  const chars = "abcdefghijklmnopqrstuvwxyz0123456789";
  let s = "";
  for (let i = 0; i < 8; i += 1) {
    s += chars[Math.floor(Math.random() * chars.length)];
  }
  return prefix ? `${prefix}_${s}` : s;
}

/**
 * 包装一个「最多只执行一次」的函数。
 * 用于事件监听注册，避免组件被多次挂载时重复订阅。
 */
export function once<T extends (...args: never[]) => unknown>(
  fn: T,
): (...args: Parameters<T>) => void {
  let called = false;
  return (...args: Parameters<T>) => {
    if (called) return;
    called = true;
    fn(...args);
  };
}

/** 延时工具，配合清理函数使用 */
export function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
