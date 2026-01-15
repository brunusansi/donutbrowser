"use client";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { LoadingButton } from "@/components/loading-button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import type { StoredProxy } from "@/types";
import { RippleButton } from "./ui/ripple";

// Advanced protocols that require Xray
const XRAY_PROTOCOLS = ["ss", "vmess", "vless", "trojan"];

interface ProxyFormData {
  name: string;
  proxy_type: string;
  host: string;
  port: number;
  username: string;
  password: string;
  proxy_url: string;
}

interface ProxyFormDialogProps {
  isOpen: boolean;
  onClose: () => void;
  editingProxy?: StoredProxy | null;
}

export function ProxyFormDialog({
  isOpen,
  onClose,
  editingProxy,
}: ProxyFormDialogProps) {
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [inputMode, setInputMode] = useState<"manual" | "url">("manual");
  const [isXrayInstalled, setIsXrayInstalled] = useState<boolean | null>(null);
  const [formData, setFormData] = useState<ProxyFormData>({
    name: "",
    proxy_type: "http",
    host: "",
    port: 8080,
    username: "",
    password: "",
    proxy_url: "",
  });

  const resetForm = useCallback(() => {
    setFormData({
      name: "",
      proxy_type: "http",
      host: "",
      port: 8080,
      username: "",
      password: "",
      proxy_url: "",
    });
    setInputMode("manual");
  }, []);

  // Check Xray installation status
  useEffect(() => {
    if (isOpen) {
      invoke<boolean>("is_xray_installed").then(setIsXrayInstalled);
    }
  }, [isOpen]);

  // Load editing proxy data when dialog opens
  useEffect(() => {
    if (isOpen) {
      if (editingProxy) {
        // Check if this is an Xray protocol URL stored in host
        const isXrayUrl =
          editingProxy.proxy_settings.host.includes("://") &&
          XRAY_PROTOCOLS.some((p) =>
            editingProxy.proxy_settings.host.startsWith(`${p}://`),
          );

        if (isXrayUrl) {
          setInputMode("url");
          setFormData({
            name: editingProxy.name,
            proxy_type: editingProxy.proxy_settings.proxy_type,
            host: "",
            port: 8080,
            username: "",
            password: "",
            proxy_url: editingProxy.proxy_settings.host,
          });
        } else {
          setInputMode("manual");
          setFormData({
            name: editingProxy.name,
            proxy_type: editingProxy.proxy_settings.proxy_type,
            host: editingProxy.proxy_settings.host,
            port: editingProxy.proxy_settings.port,
            username: editingProxy.proxy_settings.username || "",
            password: editingProxy.proxy_settings.password || "",
            proxy_url: "",
          });
        }
      } else {
        resetForm();
      }
    }
  }, [isOpen, editingProxy, resetForm]);

  // Auto-detect name from proxy URL
  const handleProxyUrlChange = useCallback(
    async (url: string) => {
      setFormData((prev) => ({ ...prev, proxy_url: url }));

      if (url.trim()) {
        try {
          const remark = await invoke<string | null>("get_proxy_remark", {
            url,
          });
          if (remark && !formData.name) {
            setFormData((prev) => ({ ...prev, name: remark }));
          }
        } catch {
          // Ignore parsing errors while typing
        }
      }
    },
    [formData.name],
  );

  const handleSubmit = useCallback(async () => {
    if (!formData.name.trim()) {
      toast.error("Proxy name is required");
      return;
    }

    // URL mode validation
    if (inputMode === "url") {
      if (!formData.proxy_url.trim()) {
        toast.error("Proxy URL is required");
        return;
      }

      // Check if it's an Xray protocol and Xray is not installed
      const isXray = await invoke<boolean>("is_xray_protocol", {
        url: formData.proxy_url,
      });
      if (isXray && !isXrayInstalled) {
        toast.error(
          "Xray is required for this protocol. Please install it in Settings > Network Engine.",
        );
        return;
      }

      setIsSubmitting(true);
      try {
        // Parse the URL to get protocol info
        const parsed = await invoke<{ protocol: string }>("parse_proxy_url", {
          url: formData.proxy_url,
        });

        // Store the URL in the host field for Xray protocols
        const proxySettings = {
          proxy_type: parsed.protocol,
          host: formData.proxy_url.trim(),
          port: 0, // Port is embedded in the URL
          username: undefined,
          password: undefined,
        };

        if (editingProxy) {
          await invoke("update_stored_proxy", {
            proxyId: editingProxy.id,
            name: formData.name.trim(),
            proxySettings,
          });
          toast.success("Proxy updated successfully");
        } else {
          await invoke("create_stored_proxy", {
            name: formData.name.trim(),
            proxySettings,
          });
          toast.success("Proxy created successfully");
        }

        onClose();
      } catch (error) {
        console.error("Failed to save proxy:", error);
        const errorMessage =
          error instanceof Error ? error.message : String(error);
        toast.error(`Failed to save proxy: ${errorMessage}`);
      } finally {
        setIsSubmitting(false);
      }
      return;
    }

    // Manual mode validation
    if (!formData.host.trim() || !formData.port) {
      toast.error("Host and port are required");
      return;
    }

    setIsSubmitting(true);
    try {
      const proxySettings = {
        proxy_type: formData.proxy_type,
        host: formData.host.trim(),
        port: formData.port,
        username: formData.username.trim() || undefined,
        password: formData.password.trim() || undefined,
      };

      if (editingProxy) {
        await invoke("update_stored_proxy", {
          proxyId: editingProxy.id,
          name: formData.name.trim(),
          proxySettings,
        });
        toast.success("Proxy updated successfully");
      } else {
        await invoke("create_stored_proxy", {
          name: formData.name.trim(),
          proxySettings,
        });
        toast.success("Proxy created successfully");
      }

      onClose();
    } catch (error) {
      console.error("Failed to save proxy:", error);
      const errorMessage =
        error instanceof Error ? error.message : String(error);
      toast.error(`Failed to save proxy: ${errorMessage}`);
    } finally {
      setIsSubmitting(false);
    }
  }, [formData, editingProxy, onClose, inputMode, isXrayInstalled]);

  const handleClose = useCallback(() => {
    if (!isSubmitting) {
      onClose();
    }
  }, [isSubmitting, onClose]);

  const isManualFormValid =
    formData.name.trim() &&
    formData.host.trim() &&
    formData.port > 0 &&
    formData.port <= 65535;

  const isUrlFormValid = formData.name.trim() && formData.proxy_url.trim();

  const isFormValid = inputMode === "url" ? isUrlFormValid : isManualFormValid;

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>
            {editingProxy ? "Edit Proxy" : "Create New Proxy"}
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-4 py-4">
          <div className="grid gap-2">
            <Label htmlFor="proxy-name">Proxy Name</Label>
            <Input
              id="proxy-name"
              value={formData.name}
              onChange={(e) =>
                setFormData({ ...formData, name: e.target.value })
              }
              placeholder="e.g. Office Proxy, Home VPN, etc."
              disabled={isSubmitting}
            />
          </div>

          <Tabs
            value={inputMode}
            onValueChange={(v) => setInputMode(v as "manual" | "url")}
          >
            <TabsList className="grid w-full grid-cols-2">
              <TabsTrigger value="manual">Manual</TabsTrigger>
              <TabsTrigger value="url">Proxy URL</TabsTrigger>
            </TabsList>

            <TabsContent value="manual" className="space-y-4 mt-4">
              <div className="grid gap-2">
                <Label>Proxy Type</Label>
                <Select
                  value={formData.proxy_type}
                  onValueChange={(value) =>
                    setFormData({ ...formData, proxy_type: value })
                  }
                  disabled={isSubmitting}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Select proxy type" />
                  </SelectTrigger>
                  <SelectContent>
                    {["http", "https", "socks4", "socks5"].map((type) => (
                      <SelectItem key={type} value={type}>
                        {type.toUpperCase()}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div className="grid gap-2">
                  <Label htmlFor="proxy-host">Host</Label>
                  <Input
                    id="proxy-host"
                    value={formData.host}
                    onChange={(e) =>
                      setFormData({ ...formData, host: e.target.value })
                    }
                    placeholder="e.g. 127.0.0.1"
                    disabled={isSubmitting}
                  />
                </div>

                <div className="grid gap-2">
                  <Label htmlFor="proxy-port">Port</Label>
                  <Input
                    id="proxy-port"
                    type="number"
                    value={formData.port}
                    onChange={(e) =>
                      setFormData({
                        ...formData,
                        port: parseInt(e.target.value, 10) || 0,
                      })
                    }
                    placeholder="e.g. 8080"
                    min="1"
                    max="65535"
                    disabled={isSubmitting}
                  />
                </div>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div className="grid gap-2">
                  <Label htmlFor="proxy-username">Username (optional)</Label>
                  <Input
                    id="proxy-username"
                    value={formData.username}
                    onChange={(e) =>
                      setFormData({
                        ...formData,
                        username: e.target.value,
                      })
                    }
                    placeholder="Proxy username"
                    disabled={isSubmitting}
                  />
                </div>

                <div className="grid gap-2">
                  <Label htmlFor="proxy-password">Password (optional)</Label>
                  <Input
                    id="proxy-password"
                    type="password"
                    value={formData.password}
                    onChange={(e) =>
                      setFormData({
                        ...formData,
                        password: e.target.value,
                      })
                    }
                    placeholder="Proxy password"
                    disabled={isSubmitting}
                  />
                </div>
              </div>
            </TabsContent>

            <TabsContent value="url" className="space-y-4 mt-4">
              <div className="grid gap-2">
                <Label htmlFor="proxy-url">Proxy URL</Label>
                <Textarea
                  id="proxy-url"
                  value={formData.proxy_url}
                  onChange={(e) => handleProxyUrlChange(e.target.value)}
                  placeholder="Paste proxy URL here (vmess://, vless://, trojan://, ss://, socks5://, http://)"
                  disabled={isSubmitting}
                  rows={3}
                  className="font-mono text-sm"
                />
                <p className="text-xs text-muted-foreground">
                  Supported protocols: VMess, VLESS, Trojan, Shadowsocks
                  (ss://), SOCKS5, HTTP
                </p>
              </div>

              {isXrayInstalled === false && (
                <Alert>
                  <AlertDescription>
                    ⚠️ Xray is not installed. VMess, VLESS, Trojan, and
                    Shadowsocks protocols require Xray. Go to Settings &gt;
                    Network Engine to install it.
                  </AlertDescription>
                </Alert>
              )}

              {isXrayInstalled === true && (
                <Alert className="border-green-500/50 bg-green-500/10">
                  <AlertDescription className="text-green-700 dark:text-green-400">
                    ✓ Xray is installed. Full protocol support available.
                  </AlertDescription>
                </Alert>
              )}
            </TabsContent>
          </Tabs>
        </div>

        <DialogFooter>
          <RippleButton
            variant="outline"
            onClick={handleClose}
            disabled={isSubmitting}
          >
            Cancel
          </RippleButton>
          <LoadingButton
            isLoading={isSubmitting}
            onClick={handleSubmit}
            disabled={!isFormValid}
          >
            {editingProxy ? "Update Proxy" : "Create Proxy"}
          </LoadingButton>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
