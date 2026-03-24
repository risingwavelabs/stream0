<script setup>
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { mountLegacyDashboard } from './legacy-dashboard'

const root = ref(null)
let cleanup = null

onMounted(() => {
  if (!root.value) return
  cleanup = mountLegacyDashboard(root.value)
})

onBeforeUnmount(() => {
  if (cleanup) cleanup()
})
</script>

<template>
  <div ref="root" />
</template>
