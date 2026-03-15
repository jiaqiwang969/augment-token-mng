<template>
  <div class="flex h-[320px] flex-col rounded-lg border border-border bg-muted/10 p-3">
    <div class="mb-2 shrink-0">
      <h4 class="m-0 text-[13px] font-semibold text-text-secondary">
        {{ $t('platform.openai.codexDialog.tokenShareTitle') }}
      </h4>
      <p class="m-0 mt-1 text-[11px] text-text-muted">
        {{ $t('platform.openai.codexDialog.tokenShareHint') }}
      </p>
    </div>

    <div
      v-if="!hasChartData"
      class="flex flex-1 flex-col items-center justify-center gap-2 text-text-muted"
    >
      <svg width="32" height="32" viewBox="0 0 24 24" fill="currentColor" class="opacity-50">
        <path
          d="M11 2v20c-5.05-.5-9-4.76-9-10s3.95-9.5 9-10zm2.03.06A10.001 10.001 0 0 1 22 12h-8.97V2.06zM13 14h8.95A10.001 10.001 0 0 1 13 21.94V14z"
        />
      </svg>
      <p class="m-0 text-[12px]">{{ $t('platform.openai.codexDialog.noData') }}</p>
    </div>

    <div v-else class="relative min-h-0 flex-1">
      <Doughnut :key="chartKey" :data="doughnutChartData" :options="chartOptions" />
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { Doughnut } from 'vue-chartjs'
import { useI18n } from 'vue-i18n'
import {
  ArcElement,
  Chart as ChartJS,
  Legend,
  Tooltip
} from 'chart.js'

ChartJS.register(ArcElement, Tooltip, Legend)

const { t: $t } = useI18n()

const props = defineProps({
  chartData: {
    type: Array,
    default: () => []
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
    legendColor: resolveCssVar('--text-secondary', isDark ? '#a3a3a3' : '#525252'),
    tooltipBg: resolveCssVar('--surface-elevated', isDark ? '#1f1f1f' : '#ffffff'),
    tooltipTitle: resolveCssVar('--text', isDark ? '#fafafa' : '#171717'),
    tooltipBody: resolveCssVar('--text-secondary', isDark ? '#a3a3a3' : '#525252'),
    tooltipBorder: resolveCssVar('--border-strong', isDark ? '#404040' : '#d4d4d4')
  }
})

const formatNumber = (v) => {
  const n = Number(v || 0)
  if (n < 1000) return n.toLocaleString()
  if (n < 1000000) return `${(n / 1000).toFixed(1).replace(/\.0$/, '')}K`
  if (n < 1000000000) return `${(n / 1000000).toFixed(2).replace(/\.00$/, '')}M`
  if (n < 1000000000000) return `${(n / 1000000000).toFixed(2).replace(/\.00$/, '')}B`
  return `${(n / 1000000000000).toFixed(2).replace(/\.00$/, '')}T`
}

const hasChartData = computed(() =>
  Array.isArray(props.chartData) && props.chartData.some(entry => Number(entry?.tokens || 0) > 0)
)

const doughnutChartData = computed(() => ({
  labels: props.chartData.map(entry => entry.label),
  datasets: [
    {
      data: props.chartData.map(entry => ({
        label: entry.label,
        memberLabel: entry.memberLabel,
        percentage: entry.percentage,
        tokens: entry.tokens
      })),
      parsing: {
        key: 'tokens'
      },
      backgroundColor: props.chartData.map(entry => entry.color || '#4c6ef5'),
      borderColor: resolveCssVar('--bg-base', currentTheme.value === 'dark' ? '#0a0a0a' : '#ffffff'),
      borderWidth: 2,
      hoverOffset: 8
    }
  ]
}))

const chartOptions = computed(() => {
  const palette = themePalette.value

  return {
    responsive: true,
    maintainAspectRatio: false,
    cutout: '62%',
    plugins: {
      legend: {
        position: 'bottom',
        labels: {
          color: palette.legendColor,
          boxWidth: 10,
          boxHeight: 10,
          usePointStyle: true,
          pointStyle: 'circle'
        }
      },
      tooltip: {
        backgroundColor: palette.tooltipBg,
        titleColor: palette.tooltipTitle,
        bodyColor: palette.tooltipBody,
        borderColor: palette.tooltipBorder,
        borderWidth: 1,
        callbacks: {
          title: (items) => items?.[0]?.label || '',
          label: (context) => {
            const memberLabel = context.raw?.memberLabel || context.label || ''
            const percentage = Number(context.raw?.percentage || 0).toFixed(1).replace(/\.0$/, '')
            const tokens = formatNumber(context.raw?.tokens || context.parsed || 0)
            return `${memberLabel}: ${tokens} ${$t('platform.openai.codexDialog.tokens')} (${percentage}%)`
          }
        }
      }
    }
  }
})

const refreshTheme = () => {
  currentTheme.value = readTheme()
  chartKey.value += 1
}

let observer = null

onMounted(() => {
  refreshTheme()

  if (typeof MutationObserver !== 'undefined' && typeof document !== 'undefined') {
    observer = new MutationObserver(() => {
      refreshTheme()
    })
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme']
    })
  }
})

onUnmounted(() => {
  if (observer) {
    observer.disconnect()
    observer = null
  }
})

watch(() => props.chartData, () => {
  chartKey.value += 1
}, { deep: true })
</script>
