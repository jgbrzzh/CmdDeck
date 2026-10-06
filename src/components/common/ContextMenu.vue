<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount } from "vue";
interface Item {
  key: string;
  label: string;
  icon?: string;
  danger?: boolean;
  disabled?: boolean;
  divider?: boolean;
  shortcut?: string;
}
const props = defineProps<{
  modelValue: boolean;
  x: number;
  y: number;
  items: Item[];
}>();
const emit = defineEmits<{
  (e: "update:modelValue", v: boolean): void;
  (e: "select", key: string): void;
}>();
const position = computed(() => ({
  left: `${Math.min(props.x, window.innerWidth - 250)}px`,
  top: `${Math.min(props.y, window.innerHeight - props.items.length * 34 - 20)}px`,
}));
function close() {
  emit("update:modelValue", false);
}
function key(e: KeyboardEvent) {
  if (e.key === "Escape") close();
}
onMounted(() => window.addEventListener("keydown", key));
onBeforeUnmount(() => window.removeEventListener("keydown", key));
</script>
<template>
  <Teleport to="body"
    ><div
      v-if="modelValue"
      class="context-backdrop"
      @mousedown.self="close"
      @contextmenu.prevent="close"
    >
      <div class="context-menu" :style="position">
        <template v-for="item in items" :key="item.key"
          ><hr v-if="item.divider" />
          <button
            v-else
            :disabled="item.disabled"
            :class="{ danger: item.danger }"
            @click="
              emit('select', item.key);
              close();
            "
          >
            {{ item.label }}<small>{{ item.shortcut }}</small>
          </button></template
        >
      </div>
    </div></Teleport
  >
</template>
<style scoped>
.context-backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
}
.context-menu {
  position: absolute;
  min-width: 235px;
  background: var(--deck-panel, #111923);
  color: var(--deck-text, #dce5ef);
  border: 1px solid var(--deck-border, #253242);
  padding: 6px;
  border-radius: 8px;
  box-shadow: 0 10px 40px #0008;
}
.context-menu button {
  display: flex;
  justify-content: space-between;
  align-items: center;
  width: 100%;
  background: none;
  color: inherit;
  border: 0;
  padding: 9px 12px;
  text-align: left;
  cursor: pointer;
  border-radius: 5px;
  font:
    12px "Microsoft YaHei",
    sans-serif;
}
.context-menu button:hover {
  background: var(--deck-card, #17212c);
}
.context-menu button:disabled {
  opacity: 0.4;
}
.context-menu button.danger {
  color: #ff7f91;
}
.context-menu hr {
  border: 0;
  border-top: 1px solid var(--deck-border, #253242);
  margin: 5px;
}
.context-menu small {
  color: var(--deck-muted, #78899d);
}
</style>
