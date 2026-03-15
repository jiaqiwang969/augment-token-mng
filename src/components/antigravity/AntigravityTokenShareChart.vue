<template>
  <div class="flex h-[320px] flex-col rounded-xl border border-border bg-muted/10 p-3">
    <div class="mb-2 shrink-0">
      <h4 class="m-0 text-[13px] font-semibold text-text-secondary">
        {{ title }}
      </h4>
      <p class="m-0 mt-1 text-[11px] text-text-muted">
        {{ hint }}
      </p>
    </div>

    <div
      v-if="loading && !hasChartData"
      class="flex flex-1 items-center justify-center text-text-muted"
    >
      <span class="spinner spinner--sm"></span>
    </div>

    <div
      v-else-if="!hasChartData"
      class="flex flex-1 flex-col items-center justify-center gap-2 text-text-muted"
    >
      <svg width="32" height="32" viewBox="0 0 24 24" fill="currentColor" class="opacity-50">
        <path
          d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zM9 17H7v-7h2v7zm4 0h-2V7h2v10zm4 0h-2v-4h2v4z"
        />
      </svg>
      <p class="m-0 text-[12px]">{{ emptyLabel }}</p>
    </div>

    <div v-else class="min-h-0 flex-1">
      <Pie :key="chartKey" :data="pieChartData" :options="chartOptions" />
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { Pie } from 'vue-chartjs'
import { ArcElement, Chart as ChartJS, Legend, Tooltip } from 'chart.js'

ChartJS.register(ArcElement, Tooltip, Legend)

const props = defineProps({
  loading: {
    type: Boolean,
    default: false
  },
  chartData: {
    type: Object,
    default: () => ({ labels: [], datasets: [] })
  },
  title: {
    type: String,
    default: ''
  },
  hint: {
    type: String,
    default: ''
  },
  emptyLabel: {
    type: String,
    default: 'No data'
  }
})

const chartKey = ref(0)
const currentTheme = ref('light')

const readTheme = () => {
  if (typeof document === 'undefined') {
    return 'light'
  }
  return document.documentElement.dataset.theme || document.documentElement.getAttribute('data-theme') || 'light'
}

const resolveCssVar = (name, fallback) => {
  if (typeof document === 'undefined') {
    return fallback
  }
  const value = getComputedStyle(document.documentElement).getPropertyValue(name)?.trim()
  return value || fallback
}

const themePalette = computed(() => {
  const isDark = currentTheme.value === 'dark'
  return {
    surface: resolveCssVar('--surface', isDark ? '#171717' : '#ffffff'),
    legendColor: resolveCssVar('--text-secondary', isDark ? '#d4d4d4' : '#404040'),
    tooltipBg: resolveCssVar('--surface-elevated', isDark ? '#1f1f1f' : '#ffffff'),
    tooltipTitle: resolveCssVar('--text', isDark ? '#fafafa' : '#171717'),
    tooltipBody: resolveCssVar('--text-secondary', isDark ? '#d4d4d4' : '#525252'),
    tooltipBorder: resolveCssVar('--border-strong', isDark ? '#404040' : '#d4d4d4')
  }
})

const hasChartData = computed(() =>
  Array.isArray(props.chartData?.labels) &&
  props.chartData.labels.length > 0 &&
  Array.isArray(props.chartData?.datasets) &&
  props.chartData.datasets[0] &&
  Array.isArray(props.chartData.datasets[0].data) &&
  props.chartData.datasets[0].data.length > 0
)

const pieChartData = computed(() => {
  if (!hasChartData.value) {
    return { labels: [], datasets: [] }
  }

  const palette = themePalette.value
  const dataset = props.chartData.datasets[0] || {}

  return {
    labels: props.chartData.labels,
    datasets: [
      {
        ...dataset,
        borderColor: palette.surface,
        hoverOffset: 14
      }
    ]
  }
})

const chartOptions = computed(() => {
  const palette = themePalette.value

  return {
    responsive: true,
    maintainAspectRatio: false,
    plugins: {
      legend: {
        position: 'bottom',
        labels: {
          padding: 14,
          boxWidth: 10,
          boxHeight: 10,
          usePointStyle: true,
          pointStyle: 'circle',
          color: palette.legendColor,
          font: {
            size: 11
          }
        }
      },
      tooltip: {
        backgroundColor: palette.tooltipBg,
        titleColor: palette.tooltipTitle,
        bodyColor: palette.tooltipBody,
        borderColor: palette.tooltipBorder,
        borderWidth: 1,
        callbacks: {
          label: (context) => {
            const label = context.label || ''
            const value = Number(context.parsed || 0)
            const total = (context.dataset.data || []).reduce((sum, item) => sum + Number(item || 0), 0)
            const percent = total > 0 ? ((value / total) * 100).toFixed(1) : '0.0'
            return `${label}: ${value.toLocaleString()} tokens (${percent}%)`
          }
        }
      }
    }
  }
})

const updateTheme = () => {
  currentTheme.value = readTheme()
}

let observer

onMounted(() => {
  updateTheme()
  if (typeof document === 'undefined') {
    return
  }
  observer = new MutationObserver(updateTheme)
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['data-theme']
  })
})

onUnmounted(() => {
  observer?.disconnect()
})

watch(currentTheme, () => {
  chartKey.value += 1
})
</script>
