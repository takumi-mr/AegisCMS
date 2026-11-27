<script setup lang="ts">
import { ref } from "vue";

const props = defineProps<{
  title: string;
  initial: number;
  step: number;
}>();

const count = ref(props.initial);
</script>

<template>
  <section class="vi-card">
    <h3 class="vi-title">{{ props.title }}</h3>
    <div class="vi-controls">
      <button type="button" class="vi-btn" @click="count -= props.step">
        -{{ props.step }}
      </button>
      <span class="vi-value" aria-live="polite">{{ count }}</span>
      <button type="button" class="vi-btn vi-btn--primary" @click="count += props.step">
        +{{ props.step }}
      </button>
    </div>
  </section>
</template>
