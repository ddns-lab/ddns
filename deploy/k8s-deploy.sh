#!/bin/bash
#
# ddnsd Kubernetes deployment script
#
# This script deploys ddnsd to a Kubernetes cluster.
#
# Usage:
#   ./k8s-deploy.sh                   # Deploy
#   ./k8s-deploy.sh --undeploy        # Remove deployment
#   ./k8s-deploy.sh --logs            # Show logs
#   ./k8s-deploy.sh --status          # Show status
#   ./k8s-deploy.sh --restart         # Restart deployment

set -e

# Configuration
NAMESPACE="ddns-system"
DEPLOYMENT_NAME="ddnsd"
SECRET_NAME="ddnsd-secrets"
CONFIGMAP_NAME="ddnsd-config"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
K8S_DIR="$SCRIPT_DIR/k8s"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Parse arguments
UNDEPLOY=false
SHOW_LOGS=false
SHOW_STATUS=false
RESTART=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --undeploy)
            UNDEPLOY=true
            shift
            ;;
        --logs)
            SHOW_LOGS=true
            shift
            ;;
        --status)
            SHOW_STATUS=true
            shift
            ;;
        --restart)
            RESTART=true
            shift
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            echo "Usage: $0 [--undeploy|--logs|--status|--restart]"
            exit 1
            ;;
    esac
done

# Undeploy
if [ "$UNDEPLOY" = true ]; then
    echo -e "${YELLOW}Removing ddnsd from Kubernetes...${NC}"

    kubectl delete -f "$K8S_DIR/deployment.yaml" --ignore-not-found=true
    kubectl delete -f "$K8S_DIR/serviceaccount.yaml" --ignore-not-found=true
    kubectl delete -f "$K8S_DIR/configmap.yaml" --ignore-not-found=true
    kubectl delete -f "$K8S_DIR/secret.yaml" --ignore-not-found=true
    kubectl delete -f "$K8S_DIR/namespace.yaml" --ignore-not-found=true

    echo -e "${GREEN}Undeploy complete.${NC}"
    exit 0
fi

# Show logs
if [ "$SHOW_LOGS" = true ]; then
    echo -e "${BLUE}Showing logs...${NC}"
    kubectl logs -f deployment/$DEPLOYMENT_NAME -n $NAMESPACE
    exit 0
fi

# Show status
if [ "$SHOW_STATUS" = true ]; then
    echo -e "${BLUE}Deployment Status:${NC}"
    echo ""
    kubectl get deployment -n $NAMESPACE
    echo ""
    echo -e "${BLUE}Pods:${NC}"
    kubectl get pods -n $NAMESPACE
    echo ""
    echo -e "${BLUE}Recent Events:${NC}"
    kubectl get events -n $NAMESPACE --sort-by='.lastTimestamp' | tail -10
    exit 0
fi

# Restart deployment
if [ "$RESTART" = true ]; then
    echo -e "${YELLOW}Restarting deployment...${NC}"
    kubectl rollout restart deployment/$DEPLOYMENT_NAME -n $NAMESPACE
    echo -e "${GREEN}Restart initiated.${NC}"
    echo "Watch status with: kubectl rollout status deployment/$DEPLOYMENT_NAME -n $NAMESPACE"
    exit 0
fi

# Deploy
echo -e "${GREEN}Deploying ddnsd to Kubernetes...${NC}"
echo ""

# Check if cluster is accessible
if ! kubectl cluster-info &> /dev/null; then
    echo -e "${RED}Error: Cannot access Kubernetes cluster.${NC}"
    echo "Please configure kubectl to access your cluster."
    exit 1
fi

# Check if secret already exists
if kubectl get secret $SECRET_NAME -n $NAMESPACE &> /dev/null; then
    echo -e "${YELLOW}Secret '$SECRET_NAME' already exists. Preserving.${NC}"
else
    # Prompt for API token
    if [ -z "$DDNS_PROVIDER_API_TOKEN" ]; then
        echo -n "Enter Cloudflare API token: "
        read -s DDNS_PROVIDER_API_TOKEN
        echo ""
    fi

    if [ -z "$DDNS_PROVIDER_API_TOKEN" ]; then
        echo -e "${RED}Error: DDNS_PROVIDER_API_TOKEN is required.${NC}"
        echo "Set it via:"
        echo "  export DDNS_PROVIDER_API_TOKEN=your_token"
        exit 1
    fi

    # Create secret
    echo "Creating secret..."
    kubectl create secret generic $SECRET_NAME \
        --from-literal=api-token="$DDNS_PROVIDER_API_TOKEN" \
        --namespace=$NAMESPACE
fi

# Prompt for records if not set
if [ -z "$DDNS_RECORDS" ]; then
    echo -n "Enter DNS records to update (comma-separated, e.g., example.com,www.example.com): "
    read DDNS_RECORDS
fi

if [ -z "$DDNS_RECORDS" ]; then
    echo -e "${RED}Error: DDNS_RECORDS is required.${NC}"
    exit 1
fi

# Update configmap with records
echo "Updating configmap..."
kubectl create configmap $CONFIGMAP_NAME \
    --from-literal=DDNS_RECORDS="$DDNS_RECORDS" \
    --namespace=$NAMESPACE \
    --dry-run=client -o yaml | kubectl apply -f -

# Apply manifests
echo "Applying manifests..."
kubectl apply -f "$K8S_DIR/namespace.yaml"
kubectl apply -f "$K8S_DIR/serviceaccount.yaml"
kubectl apply -f "$K8S_DIR/configmap.yaml"
kubectl apply -f "$K8S_DIR/deployment.yaml"

# Wait for deployment to be ready
echo ""
echo -e "${BLUE}Waiting for deployment to be ready...${NC}"
kubectl wait --for=condition=available deployment/$DEPLOYMENT_NAME -n $NAMESPACE --timeout=60s

echo ""
echo -e "${GREEN}Deployment complete!${NC}"
echo ""
echo "Check status:"
echo "  kubectl get pods -n $NAMESPACE"
echo ""
echo "View logs:"
echo "  kubectl logs -f deployment/$DEPLOYMENT_NAME -n $NAMESPACE"
echo ""
echo "Check this script for more options:"
echo "  $0 --status"
echo "  $0 --logs"
echo "  $0 --restart"
