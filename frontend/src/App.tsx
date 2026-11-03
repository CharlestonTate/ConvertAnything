import { useCallback, useEffect, useMemo, useState } from "react";
import { apiUrl } from "./api";
import { filesFromDrop, filesFromInput, normalizeExt } from "./files";
import "./App.css";

type Category = "audio" | "image" | "document";

type FormatRegistry = Record<Category, { sources: string[]; targets: string[] }>;

type JobView = {
  job_id: string;
  status: "queued" | "running" | "done" | "failed";
  category: Category;
  source_format: string;
  target_format: string;
  output_name: string;
  file_count: number;
  files_done: number;
  error?: string;
};

function categoryFromExt(registry: FormatRegistry | null, ext: string | null): Category | null {
  if (!registry || !ext) return null;
  if (registry.audio.sources.includes(ext)) return "audio";
  if (registry.image.sources.includes(ext)) return "image";
  if (registry.document.sources.includes(ext)) return "document";
  return null;
}

function sameTypeFiles(files: File[]): { files: File[]; ext: string | null; error: string | null } {
  const exts = files.map((file) => normalizeExt(file.name));
  if (files.length === 0) return { files: [], ext: null, error: null };
  if (exts.some((ext) => !ext)) {
    return { files: [], ext: null, error: "Every file needs an extension" };
  }
  const unique = [...new Set(exts as string[])];
  if (unique.length > 1) {
    return {
      files: [],
      ext: null,
      error: `Use one file type at a time (${unique.join(", ")})`,
    };
  }
  return { files, ext: unique[0], error: null };
}

export default function App() {
  const [registry, setRegistry] = useState<FormatRegistry | null>(null);
  const [files, setFiles] = useState<File[]>([]);
  const [target, setTarget] = useState("");
  const [drag, setDrag] = useState(false);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [downloadUrl, setDownloadUrl] = useState<string | null>(null);
  const [outputName, setOutputName] = useState<string | null>(null);

  const sourceExt = files[0] ? normalizeExt(files[0].name) : null;
  const category = categoryFromExt(registry, sourceExt);

  const targets = useMemo(() => {
    if (!registry || !category) return [];
    return registry[category].targets;
  }, [registry, category]);

  useEffect(() => {
    fetch(apiUrl("/api/formats"))
      .then(async (res) => {
        if (!res.ok) throw new Error("Could not load formats");
        return res.json();
      })
      .then(setRegistry)
      .catch((err: Error) => setError(err.message));
  }, []);

  useEffect(() => {
    if (targets.length === 0) {
      setTarget("");
      return;
    }
    setTarget((current) => (targets.includes(current) ? current : targets[0]));
  }, [targets]);

  const takeFiles = useCallback((next: File[]) => {
    const result = sameTypeFiles(next);
    setDownloadUrl(null);
    setOutputName(null);
    setStatus("");
    if (result.error) {
      setFiles([]);
      setError(result.error);
      return;
    }
    setError(null);
    setFiles(result.files);
  }, []);

  async function convert() {
    if (files.length === 0 || !target) return;
    setBusy(true);
    setError(null);
    setDownloadUrl(null);
    setStatus(files.length > 1 ? `Uploading ${files.length} files…` : "Uploading…");

    const body = new FormData();
    for (const file of files) {
      body.append("file", file, file.name);
    }
    body.append("target_format", target);

    try {
      const res = await fetch(apiUrl("/api/convert"), { method: "POST", body });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error ?? "Upload failed");

      const jobId = data.job_id as string;
      setStatus("Converting…");

      const job = await pollJob(jobId, setStatus);
      if (job.status === "failed") {
        throw new Error(job.error ?? "Conversion failed");
      }

      setOutputName(job.output_name);
      setDownloadUrl(apiUrl(`/api/jobs/${job.job_id}/download`));
      setStatus("");
    } catch (err) {
      const message = err instanceof Error ? err.message : "Conversion failed";
      setError(message);
      setStatus("");
    } finally {
      setBusy(false);
    }
  }

  const dropLabel =
    files.length === 0
      ? "Drop files or a folder"
      : files.length === 1
        ? files[0].name
        : `${files.length} ${sourceExt} files`;

  return (
    <div className="shell">
      <main className="stage">
        <h1>Convert Anything</h1>

        <div
          className={`dropzone ${drag ? "active" : ""}`}
          onDragOver={(e) => {
            e.preventDefault();
            setDrag(true);
          }}
          onDragLeave={() => setDrag(false)}
          onDrop={(e) => {
            e.preventDefault();
            setDrag(false);
            void filesFromDrop(e).then(takeFiles);
          }}
          onClick={() => document.getElementById("file-input")?.click()}
        >
          <p>{dropLabel}</p>
          <input
            id="file-input"
            type="file"
            hidden
            multiple
            onChange={(e) => {
              takeFiles(filesFromInput(e.target.files));
              e.target.value = "";
            }}
          />
        </div>

        <div className="controls">
          <select
            aria-label="Convert to"
            value={target}
            disabled={!category || busy}
            onChange={(e) => setTarget(e.target.value)}
          >
            {!category ? <option value="">Format</option> : null}
            {targets.map((fmt) => (
              <option key={fmt} value={fmt}>
                {fmt}
              </option>
            ))}
          </select>

          <button
            disabled={files.length === 0 || !category || !target || busy}
            onClick={() => void convert()}
          >
            {busy ? "Converting" : "Convert"}
          </button>
        </div>

        {status ? <p className="status">{status}</p> : null}
        {error ? <p className="error">{error}</p> : null}
        {downloadUrl ? (
          <a className="download" href={downloadUrl}>
            Download {outputName}
          </a>
        ) : null}
      </main>
    </div>
  );
}

async function pollJob(
  jobId: string,
  setStatus: (value: string) => void,
): Promise<JobView> {
  for (;;) {
    const res = await fetch(apiUrl(`/api/jobs/${jobId}`));
    const job = (await res.json()) as JobView & { error?: string };
    if (!res.ok) throw new Error(job.error ?? "Job lookup failed");

    if (job.status === "queued" || job.status === "running") {
      if (job.file_count > 1) {
        setStatus(`Converting ${job.files_done}/${job.file_count}`);
      } else {
        setStatus("Converting…");
      }
    }
    if (job.status === "done" || job.status === "failed") return job;

    await new Promise((resolve) => setTimeout(resolve, 700));
  }
}
