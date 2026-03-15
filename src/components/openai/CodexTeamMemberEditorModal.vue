<template>
  <BaseModal
    :visible="true"
    :title="mode === 'create' ? $t('platform.openai.codexDialog.addMember') : $t('platform.openai.codexDialog.editMember')"
    :show-close="true"
    :body-scroll="true"
    modal-class="max-w-[760px]"
    @close="$emit('close')"
  >
    <div class="space-y-4">
      <div class="grid gap-4 md:grid-cols-2">
        <div class="space-y-2">
          <label class="label mb-0">{{ $t('platform.openai.codexDialog.memberNameLabel') }}</label>
          <input
            v-model="draft.name"
            class="input"
            :disabled="busy"
            :placeholder="$t('platform.openai.codexDialog.memberNamePlaceholder')"
          />
        </div>
        <div class="space-y-2">
          <label class="label mb-0">{{ $t('platform.openai.codexDialog.memberCodeLabel') }}</label>
          <input
            v-model="draft.memberCode"
            class="input font-mono"
            :disabled="busy"
            :placeholder="$t('platform.openai.codexDialog.memberCodePlaceholder')"
          />
        </div>
      </div>

      <div class="grid gap-4 md:grid-cols-[minmax(0,1fr)_180px]">
        <div class="space-y-2">
          <label class="label mb-0">{{ $t('platform.openai.codexDialog.roleTitleLabel') }}</label>
          <input
            v-model="draft.roleTitle"
            class="input"
            :disabled="busy"
            :placeholder="$t('platform.openai.codexDialog.roleTitlePlaceholder')"
          />
        </div>
        <div class="space-y-2">
          <label class="label mb-0">{{ $t('platform.openai.codexDialog.colorLabel') }}</label>
          <div class="flex gap-2">
            <input
              v-model="draft.color"
              type="color"
              class="h-[34px] w-[46px] rounded-md border border-border bg-transparent px-1"
              :disabled="busy"
            />
            <input
              v-model="draft.color"
              class="input font-mono"
              :disabled="busy"
              placeholder="#4c6ef5"
            />
          </div>
        </div>
      </div>

      <div class="rounded-lg border border-border p-3">
        <div class="flex items-center justify-between gap-3">
          <div>
            <div class="text-sm font-semibold text-text-primary">{{ $t('platform.openai.codexDialog.memberKey') }}</div>
            <p class="mt-1 text-xs text-text-muted">
              {{ mode === 'create'
                ? $t('platform.openai.codexDialog.autoGenerateApiKeyHint')
                : $t('platform.openai.codexDialog.keySuffixLabel', { suffix: keySuffix || '-' }) }}
            </p>
          </div>
          <label class="flex items-center gap-2 text-[12px] text-text-secondary">
            <input
              v-model="draft.enabled"
              type="checkbox"
              class="h-4 w-4 accent-accent"
              :disabled="busy"
            />
            <span>{{ draft.enabled ? $t('platform.openai.codexDialog.enabledKey') : $t('platform.openai.codexDialog.disabledKey') }}</span>
          </label>
        </div>
        <div class="mt-3 flex gap-2">
          <input
            v-model="draft.apiKey"
            :type="isKeyVisible ? 'text' : 'password'"
            class="input font-mono"
            :disabled="busy"
            :placeholder="$t('platform.openai.codexDialog.apiKeyPlaceholder')"
          />
          <button
            class="btn btn--icon btn--ghost !h-[34px] !w-[34px] shrink-0"
            :disabled="busy"
            :title="isKeyVisible ? $t('platform.openai.codexDialog.hideApiKey') : $t('platform.openai.codexDialog.showApiKey')"
            @click="isKeyVisible = !isKeyVisible"
          >
            <svg v-if="isKeyVisible" width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 7c2.76 0 5 2.24 5 5 0 .65-.13 1.26-.36 1.83l2.92 2.92c1.51-1.26 2.7-2.89 3.43-4.75-1.73-4.39-6-7.5-11-7.5-1.4 0-2.74.25-3.98.7l2.16 2.16C10.74 7.13 11.35 7 12 7zM2 4.27l2.28 2.28.46.46C3.08 8.3 1.78 10.02 1 12c1.73 4.39 6 7.5 11 7.5 1.55 0 3.03-.3 4.38-.84l.42.42L19.73 22 21 20.73 3.27 3 2 4.27z"/>
            </svg>
            <svg v-else width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 4.5C7 4.5 2.73 7.61 1 12c1.73 4.39 6 7.5 11 7.5s9.27-3.11 11-7.5c-1.73-4.39-6-7.5-11-7.5zM12 17c-2.76 0-5-2.24-5-5s2.24-5 5-5 5 2.24 5 5-2.24 5-5 5zm0-8c-1.66 0-3 1.34-3 3s1.34 3 3 3 3-1.34 3-3-1.34-3-3-3z"/>
            </svg>
          </button>
        </div>
      </div>

      <div class="space-y-2">
        <label class="label mb-0">{{ $t('platform.openai.codexDialog.personaSummaryLabel') }}</label>
        <textarea
          v-model="draft.personaSummary"
          class="input min-h-[84px] resize-y"
          :disabled="busy"
          :placeholder="$t('platform.openai.codexDialog.personaSummaryPlaceholder')"
        ></textarea>
      </div>

      <div class="space-y-2">
        <label class="label mb-0">{{ $t('platform.openai.codexDialog.notesLabel') }}</label>
        <textarea
          v-model="draft.notes"
          class="input min-h-[84px] resize-y"
          :disabled="busy"
          :placeholder="$t('platform.openai.codexDialog.notesPlaceholder')"
        ></textarea>
      </div>

      <div class="rounded-lg border border-border bg-muted/10 p-3">
        <div class="text-xs font-semibold text-text-secondary">
          {{ $t('platform.openai.codexDialog.accessPreviewTitle') }}
        </div>
        <div class="mt-2 space-y-1 font-mono text-[11px] text-text-muted">
          <div>OPENAI_BASE_URL={{ publicBaseUrl }}</div>
          <div v-if="localBaseUrl"># OPENAI_BASE_URL={{ localBaseUrl }}</div>
          <div>OPENAI_API_KEY={{ draft.apiKey || 'sk-team-…' }}</div>
        </div>
      </div>
    </div>

    <template #footer>
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex flex-wrap items-center gap-2">
          <button
            v-if="mode === 'edit'"
            class="btn btn--ghost btn--sm"
            :disabled="busy"
            @click="$emit('copy-access', buildPayload())"
          >
            {{ $t('platform.openai.codexDialog.copyAccessBundle') }}
          </button>
          <button
            v-if="mode === 'edit'"
            class="btn btn--ghost btn--sm"
            :disabled="busy"
            @click="$emit('regenerate', buildPayload())"
          >
            {{ $t('platform.openai.codexDialog.generateApiKey') }}
          </button>
          <button
            v-if="mode === 'edit' && resettable"
            class="btn btn--ghost btn--sm"
            :disabled="busy"
            @click="$emit('reset-defaults', buildPayload())"
          >
            {{ $t('platform.openai.codexDialog.resetTeamDefaults') }}
          </button>
          <button
            v-if="mode === 'edit'"
            class="btn btn--ghost btn--sm text-danger"
            :disabled="busy"
            @click="$emit('delete', buildPayload())"
          >
            {{ $t('common.delete') }}
          </button>
        </div>
        <div class="flex items-center gap-2">
          <button
            class="btn btn--secondary btn--sm"
            :disabled="busy"
            @click="$emit('close')"
          >
            {{ $t('common.cancel') }}
          </button>
          <button
            class="btn btn--primary btn--sm"
            :disabled="busy"
            @click="$emit('save', buildPayload())"
          >
            <span v-if="busy" class="btn-spinner mr-2" aria-hidden="true"></span>
            {{ $t('common.save') }}
          </button>
        </div>
      </div>
    </template>
  </BaseModal>
</template>

<script setup>
import { computed, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import BaseModal from '@/components/common/BaseModal.vue'

const props = defineProps({
  mode: {
    type: String,
    default: 'create'
  },
  member: {
    type: Object,
    required: true
  },
  busy: {
    type: Boolean,
    default: false
  },
  resettable: {
    type: Boolean,
    default: false
  },
  publicBaseUrl: {
    type: String,
    default: ''
  },
  localBaseUrl: {
    type: String,
    default: ''
  }
})

defineEmits(['close', 'save', 'regenerate', 'delete', 'copy-access', 'reset-defaults'])

const { t: $t } = useI18n()
const isKeyVisible = ref(false)

const createDraft = (member) => ({
  id: String(member?.id || '').trim(),
  name: String(member?.name || '').trim(),
  apiKey: String(member?.apiKey || '').trim(),
  enabled: member?.enabled !== false,
  memberCode: String(member?.memberCode || '').trim(),
  roleTitle: String(member?.roleTitle || '').trim(),
  personaSummary: String(member?.personaSummary || '').trim(),
  color: String(member?.color || '#4c6ef5').trim() || '#4c6ef5',
  notes: String(member?.notes || '').trim()
})

const draft = reactive(createDraft(props.member))

watch(
  () => props.member,
  (member) => {
    Object.assign(draft, createDraft(member))
    isKeyVisible.value = props.mode === 'create'
  },
  { immediate: true, deep: true }
)

const keySuffix = computed(() => {
  const trimmed = String(draft.apiKey || '').trim()
  if (!trimmed) {
    return ''
  }

  const segments = trimmed.split('-').filter(Boolean)
  return segments[segments.length - 1] || trimmed.slice(-8)
})

const normalizeOptionalField = (value) => {
  const trimmed = String(value || '').trim()
  return trimmed || null
}

const buildPayload = () => ({
  id: draft.id,
  name: String(draft.name || '').trim(),
  apiKey: normalizeOptionalField(draft.apiKey),
  enabled: draft.enabled !== false,
  memberCode: String(draft.memberCode || '').trim(),
  roleTitle: normalizeOptionalField(draft.roleTitle),
  personaSummary: normalizeOptionalField(draft.personaSummary),
  color: normalizeOptionalField(draft.color),
  notes: normalizeOptionalField(draft.notes)
})
</script>
