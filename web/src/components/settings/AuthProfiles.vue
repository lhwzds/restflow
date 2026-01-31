<script setup lang="ts">
/**
 * Auth Profiles Management Component
 * 
 * Manages authentication profiles for LLM providers with:
 * - Auto-discovery from Claude Code, environment, keychain
 * - Manual profile creation
 * - Health tracking and status display
 */

import { ref, computed, onMounted } from 'vue';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import {
  authInitialize,
  authListProfiles,
  authAddProfile,
  authRemoveProfile,
  authEnableProfile,
  authDisableProfile,
  authDiscover,
  authGetSummary,
  type ManagerSummary,
  type AddProfileRequest,
} from '@/api/auth';
import type { AuthProfile, AuthProvider, Credential } from '@/types/generated';

// State
const profiles = ref<AuthProfile[]>([]);
const summary = ref<ManagerSummary | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);
const showAddDialog = ref(false);

// New profile form
const newProfile = ref<AddProfileRequest>({
  name: '',
  api_key: '',
  provider: 'anthropic' as AuthProvider,
  email: undefined,
  priority: 0,
});

// Computed
const groupedProfiles = computed(() => {
  const grouped: Record<string, AuthProfile[]> = {};
  for (const profile of profiles.value) {
    const provider = profile.provider;
    if (!grouped[provider]) {
      grouped[provider] = [];
    }
    grouped[provider].push(profile);
  }
  return grouped;
});

// Methods
async function loadProfiles() {
  loading.value = true;
  error.value = null;
  try {
    // Initialize if needed
    await authInitialize();
    profiles.value = await authListProfiles();
    summary.value = await authGetSummary();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

async function runDiscovery() {
  loading.value = true;
  error.value = null;
  try {
    await authDiscover();
    await loadProfiles();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

async function addProfile() {
  if (!newProfile.value.name || !newProfile.value.api_key) {
    error.value = 'Name and API key are required';
    return;
  }

  loading.value = true;
  error.value = null;
  try {
    const response = await authAddProfile(newProfile.value);
    if (!response.success) {
      error.value = response.error || 'Failed to add profile';
      return;
    }
    showAddDialog.value = false;
    newProfile.value = {
      name: '',
      api_key: '',
      provider: 'anthropic' as AuthProvider,
      email: undefined,
      priority: 0,
    };
    await loadProfiles();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

async function removeProfile(profileId: string) {
  if (!confirm('Are you sure you want to remove this profile?')) return;
  
  loading.value = true;
  error.value = null;
  try {
    const response = await authRemoveProfile(profileId);
    if (!response.success) {
      error.value = response.error || 'Failed to remove profile';
      return;
    }
    await loadProfiles();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

async function toggleProfile(profile: AuthProfile) {
  loading.value = true;
  error.value = null;
  try {
    if (profile.enabled) {
      const response = await authDisableProfile(profile.id, 'User disabled');
      if (!response.success) {
        error.value = response.error || 'Failed to disable profile';
        return;
      }
    } else {
      const response = await authEnableProfile(profile.id);
      if (!response.success) {
        error.value = response.error || 'Failed to enable profile';
        return;
      }
    }
    await loadProfiles();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

function getHealthBadgeVariant(health: string): 'default' | 'secondary' | 'destructive' | 'outline' {
  switch (health) {
    case 'healthy':
      return 'default';
    case 'cooldown':
      return 'secondary';
    case 'disabled':
      return 'destructive';
    default:
      return 'outline';
  }
}

function getSourceIcon(source: string): string {
  switch (source) {
    case 'claude_code':
      return '🤖';
    case 'keychain':
      return '🔐';
    case 'environment':
      return '🌍';
    case 'manual':
      return '✏️';
    default:
      return '❓';
  }
}

function maskApiKey(key: string): string {
  if (key.length <= 8) return '*'.repeat(key.length);
  return `${key.slice(0, 4)}...${key.slice(-4)}`;
}

/**
 * Safely extract the displayable credential value from a Credential union type
 */
function getCredentialDisplayValue(credential: Credential): string {
  switch (credential.type) {
    case 'api_key':
      return credential.key;
    case 'token':
      return credential.token;
    case 'o_auth':
      return credential.access_token;
    default:
      return '';
  }
}

// Lifecycle
onMounted(loadProfiles);
</script>

<template>
  <div class="space-y-6">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-2xl font-bold tracking-tight">Auth Profiles</h2>
        <p class="text-muted-foreground">
          Manage authentication credentials for LLM providers
        </p>
      </div>
      <div class="flex gap-2">
        <Button variant="outline" @click="runDiscovery" :disabled="loading">
          🔍 Discover
        </Button>
        <Dialog v-model:open="showAddDialog">
          <DialogTrigger as-child>
            <Button>➕ Add Profile</Button>
          </DialogTrigger>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Add Auth Profile</DialogTitle>
              <DialogDescription>
                Add a manual API key for an LLM provider
              </DialogDescription>
            </DialogHeader>
            <div class="grid gap-4 py-4">
              <div class="grid gap-2">
                <Label for="name">Name</Label>
                <Input
                  id="name"
                  v-model="newProfile.name"
                  placeholder="My Anthropic Key"
                />
              </div>
              <div class="grid gap-2">
                <Label for="provider">Provider</Label>
                <Select v-model="newProfile.provider">
                  <SelectTrigger>
                    <SelectValue placeholder="Select provider" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="anthropic">Anthropic</SelectItem>
                    <SelectItem value="openai">OpenAI</SelectItem>
                    <SelectItem value="google">Google</SelectItem>
                    <SelectItem value="other">Other</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div class="grid gap-2">
                <Label for="api_key">API Key</Label>
                <Input
                  id="api_key"
                  v-model="newProfile.api_key"
                  type="password"
                  placeholder="sk-ant-api03-..."
                />
              </div>
              <div class="grid gap-2">
                <Label for="email">Email (optional)</Label>
                <Input
                  id="email"
                  v-model="newProfile.email"
                  type="email"
                  placeholder="user@example.com"
                />
              </div>
            </div>
            <DialogFooter>
              <Button variant="outline" @click="showAddDialog = false">
                Cancel
              </Button>
              <Button @click="addProfile" :disabled="loading">
                Add Profile
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>
    </div>

    <!-- Error Alert -->
    <div
      v-if="error"
      class="bg-destructive/10 border border-destructive text-destructive px-4 py-3 rounded-lg"
    >
      {{ error }}
    </div>

    <!-- Summary Cards -->
    <div v-if="summary" class="grid gap-4 md:grid-cols-4">
      <Card>
        <CardHeader class="pb-2">
          <CardTitle class="text-sm font-medium">Total Profiles</CardTitle>
        </CardHeader>
        <CardContent>
          <div class="text-2xl font-bold">{{ summary.total }}</div>
        </CardContent>
      </Card>
      <Card>
        <CardHeader class="pb-2">
          <CardTitle class="text-sm font-medium">Available</CardTitle>
        </CardHeader>
        <CardContent>
          <div class="text-2xl font-bold text-green-600">{{ summary.available }}</div>
        </CardContent>
      </Card>
      <Card>
        <CardHeader class="pb-2">
          <CardTitle class="text-sm font-medium">In Cooldown</CardTitle>
        </CardHeader>
        <CardContent>
          <div class="text-2xl font-bold text-yellow-600">{{ summary.in_cooldown }}</div>
        </CardContent>
      </Card>
      <Card>
        <CardHeader class="pb-2">
          <CardTitle class="text-sm font-medium">Disabled</CardTitle>
        </CardHeader>
        <CardContent>
          <div class="text-2xl font-bold text-red-600">{{ summary.disabled }}</div>
        </CardContent>
      </Card>
    </div>

    <!-- Loading State -->
    <div v-if="loading" class="flex items-center justify-center py-8">
      <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
    </div>

    <!-- Profiles by Provider -->
    <div v-else class="space-y-6">
      <div v-for="(providerProfiles, provider) in groupedProfiles" :key="provider">
        <h3 class="text-lg font-semibold mb-3 capitalize">{{ provider }}</h3>
        <div class="grid gap-4">
          <Card
            v-for="profile in providerProfiles"
            :key="profile.id"
            :class="{ 'opacity-50': !profile.enabled }"
          >
            <CardHeader class="pb-2">
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-2">
                  <span class="text-lg">{{ getSourceIcon(profile.source) }}</span>
                  <CardTitle class="text-base">{{ profile.name }}</CardTitle>
                </div>
                <div class="flex items-center gap-2">
                  <Badge :variant="getHealthBadgeVariant(profile.health)">
                    {{ profile.health }}
                  </Badge>
                  <Badge variant="outline">
                    {{ profile.source.replace('_', ' ') }}
                  </Badge>
                </div>
              </div>
              <CardDescription v-if="profile.credential">
                {{ maskApiKey(getCredentialDisplayValue(profile.credential)) }}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div class="flex items-center justify-between">
                <div class="text-sm text-muted-foreground">
                  <span v-if="profile.last_used_at">
                    Last used: {{ new Date(profile.last_used_at).toLocaleDateString() }}
                  </span>
                  <span v-else>Never used</span>
                  <span v-if="profile.failure_count > 0" class="ml-2 text-yellow-600">
                    ({{ profile.failure_count }} failures)
                  </span>
                </div>
                <div class="flex gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    @click="toggleProfile(profile)"
                  >
                    {{ profile.enabled ? '⏸️ Disable' : '▶️ Enable' }}
                  </Button>
                  <Button
                    v-if="profile.source === 'manual'"
                    variant="destructive"
                    size="sm"
                    @click="removeProfile(profile.id)"
                  >
                    🗑️ Remove
                  </Button>
                </div>
              </div>
            </CardContent>
          </Card>
        </div>
      </div>

      <!-- Empty State -->
      <div
        v-if="profiles.length === 0"
        class="text-center py-12 text-muted-foreground"
      >
        <p class="text-lg mb-2">No auth profiles found</p>
        <p class="text-sm">
          Click "Discover" to find credentials or "Add Profile" to add one manually
        </p>
      </div>
    </div>
  </div>
</template>
