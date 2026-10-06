/**
 * 窗口尺寸断点 composable。
 *
 * 应用外框是 CSS Grid + 固定宽度的侧栏/面板，窗口变窄时需要主动
 * 折叠侧栏、收窄分组面板，所以这里把宽度做成响应式的。
 *
 * 断点：
 * - `isMobile`  < 820px   （极窄，只保留图标侧栏）
 * - `isNarrow`  < 1100px  （自动折叠侧栏，任务书指定值）
 * - `isCompact` < 1400px  （收窄分组面板）
 */

import {
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  type ComputedRef,
  type Ref,
} from "vue";

export interface UseResponsiveReturn {
  width: Ref<number>;
  height: Ref<number>;
  /** < 1100px */
  isNarrow: Ref<boolean>;
  /** < 1400px */
  isCompact: Ref<boolean>;
  /** < 820px */
  isMobile: Ref<boolean>;
  /** 三个断点打包，方便模板里一次性解构 */
  breakpoints: ComputedRef<{
    narrow: boolean;
    compact: boolean;
    mobile: boolean;
  }>;
  /** 手动调用一次即可立即刷新（配合 window resize 已在内部自动调用） */
  refresh(): void;
}

const width = ref(typeof window === "undefined" ? 1440 : window.innerWidth);
const height = ref(typeof window === "undefined" ? 900 : window.innerHeight);

const isNarrow = computed(() => width.value < 1100);
const isCompact = computed(() => width.value < 1400);
const isMobile = computed(() => width.value < 820);

const breakpoints = computed(() => ({
  narrow: isNarrow.value,
  compact: isCompact.value,
  mobile: isMobile.value,
}));

/** 从 window 读一次当前尺寸 */
function refresh(): void {
  if (typeof window === "undefined") return;
  width.value = window.innerWidth;
  height.value = window.innerHeight;
}

/**
 * 在 `onMounted` 订阅 resize / orientationchange，`onBeforeUnmount` 退订。
 * 组件外调用（比如 main.ts 里）也不会报错，只是不会自动清理。
 */
export function useResponsive(): UseResponsiveReturn {
  let handler: (() => void) | null = null;
  let mediaHandler: (() => void) | null = null;

  onMounted(() => {
    refresh();

    handler = () => refresh();
    window.addEventListener("resize", handler, { passive: true });
    window.addEventListener("orientationchange", handler, { passive: true });

    // Tauri 窗口缩放有时不触发 resize，额外监听 VisualViewport（WebView2 支持）
    if (window.visualViewport) {
      mediaHandler = () => refresh();
      window.visualViewport.addEventListener("resize", mediaHandler);
    }
  });

  onBeforeUnmount(() => {
    if (handler) {
      window.removeEventListener("resize", handler);
      window.removeEventListener("orientationchange", handler);
      handler = null;
    }
    if (mediaHandler && window.visualViewport) {
      window.visualViewport.removeEventListener("resize", mediaHandler);
      mediaHandler = null;
    }
  });

  return { width, height, isNarrow, isCompact, isMobile, breakpoints, refresh };
}
