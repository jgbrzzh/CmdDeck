<script setup lang="ts">
withDefaults(
  defineProps<{ icon?: string; title?: string; description?: string }>(),
  { icon: "terminal", title: "暂无内容", description: "" },
);
</script>
<template>
  <div class="empty-state">
    <div class="empty-state__symbol">&gt;_</div>
    <h2>{{ title }}</h2>
    <p>{{ description }}</p>
    <slot name="action" />
  </div>
</template>
<style scoped>
.empty-state {
  text-align: center;
  max-width: 520px;
  margin: auto;
  padding: 40px;
  color: var(--text-secondary, #78899d);
}
.empty-state__symbol {
  font:
    64px Consolas,
    monospace;
  color: #4edca7;
  margin-bottom: 25px;
  opacity: 0.7;
}
.empty-state h2 {
  font-size: 21px;
  color: var(--text-primary, #dce5ef);
}
.empty-state p {
  line-height: 1.9;
  margin: 16px 0 28px;
}
</style>
