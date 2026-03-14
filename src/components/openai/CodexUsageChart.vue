<template>
  <div class="flex h-[320px] flex-col rounded-lg border border-border bg-muted/10 p-3">
    <div class="mb-2 flex shrink-0 items-center justify-between gap-3">
      <div>
        <h4 class="m-0 text-[13px] font-semibold text-text-secondary">
          {{ $t('platform.openai.codexDialog.monthlyTrend') }}
        </h4>
        <p class="m-0 mt-1 text-[11px] text-text-muted">
          {{ $t('platform.openai.codexDialog.monthlyTrendHint') }}
        </p>
      </div>
      <div class="flex items-center gap-3 text-[11px]">
        <button
          class="flex items-center gap-1 transition-opacity hover:opacity-70"
          :class="activeMetric === 'both' ? 'opacity-100' : 'opacity-40'"
          @click="activeMetric = 'both'"
        >
          <span class="flex items-center gap-1">
            <span class="h-2 w-3 rounded-full bg-[#4c6ef5]"></span>
            <span class="h-2 w-3 rounded-full border border-[#4c6ef5] bg-transparent"></span>
          </span>
          {{ $t('platform.openai.codexDialog.requests') }}/{{ $t('platform.openai.codexDialog.tokens') }}
        </button>
        <button
          class="flex items-center gap-1 transition-opacity hover:opacity-70"
          :class="activeMetric === 'requests' ? 'opacity-100' : 'opacity-40'"
          @click="activeMetric = 'requests'"
        >
          <span class="h-2.5 w-2.5 rounded-full bg-[#4c6ef5]"></span>
          {{ $t('platform.openai.codexDialog.requests') }}
        </button>
        <button
          class="flex items-center gap-1 transition-opacity hover:opacity-70"
          :class="activeMetric === 'tokens' ? 'opacity-100' : 'opacity-40'"
          @click="activeMetric = 'tokens'"
        >
          <span class="h-2.5 w-2.5 rounded-full border border-[#4c6ef5] bg-transparent"></span>
          {{ $t('platform.openai.codexDialog.tokens') }}
        </button>
      </div>
    </div>

    <div
      v-if="loading && !hasChartData"
      class="flex flex-1 flex-col items-center justify-center gap-2 text-text-muted"
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
      <p class="m-0 text-[12px]">{{ $t('platform.openai.codexDialog.noData') }}</p>
    </div>

    <div v-else class="relative min-h-0 flex-1">
      <Line :key="chartKey" :data="lineChartData" :options="chartOptions" />
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { Line } from 'vue-chartjs'
import { useI18n } from 'vue-i18n'
import {
  CategoryScale,
  Chart as ChartJS,
  Legend,
  LineElement,
  LinearScale,
  PointElement,
  Tooltip
} from 'chart.js'

ChartJS.register(CategoryScale, LinearScale, PointElement, LineElement, Tooltip, Legend)

const { t: $t } = useI18n()

const KEY_COLORS = [
  '#4c6ef5',
  '#f06595',
  '#2f9e44',
  '#f08c00',
  '#7c3aed',
  '#0ea5e9',
  '#d6336c',
  '#1d4ed8',
  '#16a34a',
  '#c2410c'
]

const props = defineProps({
  loading: {
    type: Boolean,
    default: false
  },
  chartData: {
    type: Array,
    default: () => []
  }
})

const activeMetric = ref('requests')
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
  const theme = currentTheme.value
  const isDark = theme === 'dark'

  return {
    gridColor: resolveCssVar('--border', isDark ? '#404040' : '#e5e5e5'),
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

const formatLabel = (dateStr) => {
  const date = new Date(dateStr)
  if (Number.isNaN(date.getTime())) return dateStr
  return `${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
}

const normalizedSeries = computed(() => {
  if (!Array.isArray(props.chartData) || props.chartData.length === 0) {
    return []
  }

  const hasSeriesShape = props.chartData.some(entry => Array.isArray(entry?.stats))
  if (!hasSeriesShape) {
    return [
      {
        profileId: 'all',
        profileName: 'All Keys',
        stats: props.chartData
      }
    ]
  }

  return props.chartData
    .filter(entry => Array.isArray(entry?.stats))
    .map((entry, index) => ({
      profileId: entry.profileId || `profile-${index + 1}`,
      profileName: entry.profileName || entry.name || `Key ${index + 1}`,
      memberCode: entry.memberCode || '',
      roleTitle: entry.roleTitle || '',
      color: entry.color || '',
      stats: entry.stats
    }))
})

const formatSeriesLabel = (series) => {
  const parts = [series.profileName, series.memberCode, series.roleTitle]
    .map(value => String(value || '').trim())
    .filter(Boolean)
  return parts[0] ? parts.join(' · ') : series.profileName
}

const labels = computed(() => {
  const firstSeries = normalizedSeries.value.find(series => series.stats.length > 0)
  return firstSeries ? firstSeries.stats.map(point => formatLabel(point.date)) : []
})

const hasChartData = computed(() =>
  normalizedSeries.value.some(series =>
    series.stats.some(point => Number(point.requests || 0) > 0 || Number(point.tokens || 0) > 0)
  )
)

const buildDataset = (series, index, metric) => {
  const color = series.color || KEY_COLORS[index % KEY_COLORS.length]
  const isRequests = metric === 'requests'

  return {
    label: `${formatSeriesLabel(series)} · ${isRequests ? $t('platform.openai.codexDialog.requests') : $t('platform.openai.codexDialog.tokens')}`,
    data: series.stats.map(point => (isRequests ? point.requests || 0 : point.tokens || 0)),
    yAxisID: isRequests ? 'yRequests' : 'yTokens',
    borderColor: color,
    backgroundColor: color,
    borderWidth: 2,
    borderDash: isRequests ? [] : [8, 5],
    tension: 0.28,
    pointRadius: 2,
    pointHoverRadius: 5,
    fill: false
  }
}

const lineChartData = computed(() => {
  if (!hasChartData.value) {
    return { labels: [], datasets: [] }
  }

  const datasets = []
  normalizedSeries.value.forEach((series, index) => {
    if (activeMetric.value === 'requests' || activeMetric.value === 'both') {
      datasets.push(buildDataset(series, index, 'requests'))
    }
    if (activeMetric.value === 'tokens' || activeMetric.value === 'both') {
      datasets.push(buildDataset(series, index, 'tokens'))
    }
  })

  return {
    labels: labels.value,
    datasets
  }
})

const chartOptions = computed(() => {
  const palette = themePalette.value
  const isRequests = activeMetric.value === 'requests'
  const isTokens = activeMetric.value === 'tokens'
  const isBoth = activeMetric.value === 'both'

  const createAxis = ({ position = 'left', display = true, drawOnChartArea = true }) => ({
    display,
    position,
    grid: {
      color: palette.gridColor,
      drawBorder: false,
      drawOnChartArea
    },
    ticks: {
      color: palette.legendColor,
      font: {
        size: 10
      },
      precision: 0,
      callback: value => {
        if (value < 1) return ''
        return formatNumber(value)
      }
    },
    min: 0,
    beginAtZero: true
  })

  return {
    responsive: true,
    maintainAspectRatio: false,
    interaction: {
      mode: 'index',
      intersect: false
    },
    plugins: {
      legend: {
        display: true,
        position: 'top',
        align: 'end',
        labels: {
          color: palette.legendColor,
          boxWidth: 10,
          boxHeight: 10,
          usePointStyle: true,
          pointStyle: 'line',
          padding: 12,
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
        padding: 10,
        displayColors: true,
        callbacks: {
          title: items => {
            if (items.length > 0) {
              const index = items[0].dataIndex
              const firstSeries = normalizedSeries.value.find(series => series.stats[index])
              return firstSeries?.stats[index]?.date || ''
            }
            return ''
          },
          label: context => `${context.dataset?.label || ''}: ${formatNumber(context.parsed?.y || 0)}`
        }
      }
    },
    scales: {
      x: {
        grid: {
          display: false
        },
        ticks: {
          color: palette.legendColor,
          font: {
            size: 10
          },
          maxRotation: 0,
          autoSkip: true,
          maxTicksLimit: 7
        }
      },
      yRequests: createAxis({
        position: 'left',
        display: isRequests || isBoth,
        drawOnChartArea: true
      }),
      yTokens: createAxis({
        position: 'right',
        display: isTokens || isBoth,
        drawOnChartArea: isBoth
      })
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

watch(activeMetric, () => {
  chartKey.value += 1
})

watch(
  () => props.chartData,
  () => {
    chartKey.value += 1
  },
  { deep: true }
)
</script>
