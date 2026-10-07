<script setup lang="ts">
import Icon from "./Icon.vue";
import { ICON_PATHS } from "./icons";
defineProps<{ name: string }>();
</script>
<template>
  <span class="preset-icon" :title="name" :data-icon="name">
    <Icon v-if="name in ICON_PATHS" :name="name" :size="20" />
    <span v-else>{{ name.slice(0, 12) || "›_" }}</span>
  </span>
</template>
<style scoped>
.preset-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 28px;
  max-width: 100px;
  overflow: hidden;
  color: var(--cd-accent);
}
</style>
