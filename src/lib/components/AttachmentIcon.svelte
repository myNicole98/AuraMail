<script lang="ts">
  import {
    File,
    FileArchive,
    FileAudio,
    FileCode,
    FileImage,
    FileSpreadsheet,
    FileText,
    FileVideo
  } from "lucide-svelte";
  import { attachmentKind, type AttachmentKind } from "../attachmentIcon";

  let {
    filename,
    mimeType,
    size = 14
  }: { filename: string; mimeType: string; size?: number } = $props();

  let kind = $derived(attachmentKind(mimeType, filename));
  const icons: Record<AttachmentKind, typeof FileText> = {
    pdf: FileText,
    image: FileImage,
    archive: FileArchive,
    spreadsheet: FileSpreadsheet,
    audio: FileAudio,
    video: FileVideo,
    code: FileCode,
    text: FileText,
    file: File
  };
  let Icon = $derived(icons[kind]);
</script>

<span class="attachment-icon {kind}" data-attachment-kind={kind} aria-hidden="true">
  <Icon {size} />
</span>

<style>
  .attachment-icon {
    display: grid;
    flex: none;
    place-items: center;
  }
  .pdf {
    color: var(--danger);
  }
  .image {
    color: var(--info);
  }
  .archive {
    color: var(--warning);
  }
  .spreadsheet {
    color: var(--success);
  }
  .audio,
  .video {
    color: var(--accent-alt);
  }
  .code {
    color: var(--accent);
  }
  .text,
  .file {
    color: var(--muted);
  }
</style>
