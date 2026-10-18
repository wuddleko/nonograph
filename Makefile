.PHONY: help build up down logs clean dev dev-down dev-logs test restart status onion check-docker install-docker

# Default target
help:
	@echo "Quick Commands:"
	@echo "  up          - Start application (checks for Docker first)"
	@echo "  down        - Stop application"
	@echo "  logs        - View logs"
	@echo "  status      - Check status"
	@echo "  onion       - Show .onion URL"
	@echo "  clean       - Remove this project's containers, images, and volumes"
	@echo ""
	@echo "Docker Management:"
	@echo "  check-docker     - Check if Docker is installed"
	@echo "  install-docker   - Install Docker on this system"
	@echo ""
	@echo "Note: All commands will automatically check for Docker and offer to install it if missing."
	@echo ""
	@echo "Native Debian install: ./scripts/run"
	@echo "Native service control: ./scripts/nostorgraph.sh"
	@echo "Native status: ./scripts/status.sh"

# Check if Docker and Docker Compose are installed
check-docker:
	@echo "Checking Docker installation..."
	@if ! command -v docker >/dev/null 2>&1; then \
		echo "❌ Docker is not installed on this system."; \
		echo "You need Docker to run Nostorgraph. Would you like to install it? (y/N)"; \
		read -r response; \
		if [ "$$response" = "y" ] || [ "$$response" = "Y" ]; then \
			$(MAKE) install-docker; \
		else \
			echo "Docker installation cancelled. Please install Docker manually and try again."; \
			echo "Visit: https://docs.docker.com/get-docker/"; \
			exit 1; \
		fi; \
	else \
		echo "✅ Docker is installed"; \
	fi
	@if ! command -v docker compose >/dev/null 2>&1 && ! command -v docker-compose >/dev/null 2>&1; then \
		echo "❌ Docker Compose is not installed on this system."; \
		echo "You need Docker Compose to run Nostorgraph. Would you like to install it? (y/N)"; \
		read -r response; \
		if [ "$$response" = "y" ] || [ "$$response" = "Y" ]; then \
			$(MAKE) install-docker-compose; \
		else \
			echo "Docker Compose installation cancelled. Please install Docker Compose manually and try again."; \
			echo "Visit: https://docs.docker.com/compose/install/"; \
			exit 1; \
		fi; \
	else \
		echo "✅ Docker Compose is installed"; \
	fi

# Install Docker (works on most Linux distributions)
install-docker:
	@echo "Installing Docker..."
	@if command -v apt >/dev/null 2>&1; then \
		if grep -q "^ID=debian" /etc/os-release; then \
			echo "Detected Debian system, installing via apt..."; \
			DOCKER_OS="debian"; \
			FALLBACK_CODENAME="bookworm"; \
		elif grep -q "^ID=ubuntu" /etc/os-release || grep -q "^ID_LIKE.*ubuntu" /etc/os-release; then \
			echo "Detected Ubuntu-based system, installing via apt..."; \
			DOCKER_OS="ubuntu"; \
			FALLBACK_CODENAME="jammy"; \
		else \
			echo "Detected Debian/Ubuntu-like system, defaulting to Ubuntu repository..."; \
			DOCKER_OS="ubuntu"; \
			FALLBACK_CODENAME="jammy"; \
		fi; \
		CODENAME=$$(lsb_release -cs); \
		sudo apt update && \
		sudo apt install -y ca-certificates curl gnupg lsb-release && \
		sudo mkdir -p /etc/apt/keyrings && \
		curl -fsSL https://download.docker.com/linux/$$DOCKER_OS/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg && \
		echo "deb [arch=$$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/$$DOCKER_OS $$CODENAME stable" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null && \
		if ! sudo apt update 2>/dev/null; then \
			echo "Warning: Repository for $$CODENAME not found, falling back to $$FALLBACK_CODENAME..."; \
			echo "deb [arch=$$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/$$DOCKER_OS $$FALLBACK_CODENAME stable" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null && \
			sudo apt update; \
		fi && \
		sudo apt install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin && \
		sudo usermod -aG docker $$USER && \
		echo "✅ Docker installed successfully! Please log out and back in to use Docker without sudo."; \
	elif command -v yum >/dev/null 2>&1; then \
		echo "Detected RHEL/CentOS/Fedora system, installing via yum..."; \
		sudo yum install -y yum-utils && \
		sudo yum-config-manager --add-repo https://download.docker.com/linux/centos/docker-ce.repo && \
		sudo yum install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin && \
		sudo systemctl start docker && \
		sudo systemctl enable docker && \
		sudo usermod -aG docker $$USER && \
		echo "✅ Docker installed successfully! Please log out and back in to use Docker without sudo."; \
	elif command -v pacman >/dev/null 2>&1; then \
		echo "Detected Arch Linux system, installing via pacman..."; \
		sudo pacman -S --noconfirm docker docker-compose && \
		sudo systemctl start docker && \
		sudo systemctl enable docker && \
		sudo usermod -aG docker $$USER && \
		echo "✅ Docker installed successfully! Please log out and back in to use Docker without sudo."; \
	else \
		echo "❌ Unsupported package manager. Please install Docker manually:"; \
		echo "Visit: https://docs.docker.com/get-docker/"; \
		exit 1; \
	fi

# Install Docker Compose (fallback for systems without compose plugin)
install-docker-compose:
	@echo "Installing Docker Compose..."
	@sudo curl -L "https://github.com/docker/compose/releases/latest/download/docker-compose-$$(uname -s)-$$(uname -m)" -o /usr/local/bin/docker-compose
	@sudo chmod +x /usr/local/bin/docker-compose
	@echo "✅ Docker Compose installed successfully!"

# Production targets
build: check-docker
	@echo "Building Nostorgraph Docker images..."
	sudo docker compose build

up: check-docker
	@echo "Starting Nostorgraph application..."
	sudo docker compose up -d
	@echo "Application started! Access it at:"
	@echo "  Local: http://localhost:8009"
	@echo ""
	@echo "Getting Tor hidden service address..."
	@echo "Waiting for Tor to bootstrap (this may take 30-60 seconds)..."
	@sleep 30
	@echo "Your .onion address:"
	@sudo docker exec -u debian-tor nostorgraph_app cat /var/lib/tor/hidden_service/hostname 2>/dev/null || echo "Tor address not ready yet, check logs with 'make logs'"
	@echo ""
	@echo "Save this address to share your anonymous publishing platform!"

down: check-docker
	@echo "Stopping Nostorgraph application..."
	sudo docker compose down

logs: check-docker
	sudo docker compose logs -f

restart: check-docker
	@echo "Restarting Nostorgraph application..."
	sudo docker compose restart
	@echo "Application restarted!"

status: check-docker
	@echo "Container Status:"
	@echo "=================="
	sudo docker compose ps
	@echo ""
	@echo "Tor Hidden Service Address:"
	@echo "=========================="
	@sudo docker exec -u debian-tor nostorgraph_app cat /var/lib/tor/hidden_service/hostname 2>/dev/null || echo "No Tor address found"

# Development targets
dev: check-docker
	@echo "Starting Nostorgraph development environment..."
	sudo docker compose -f docker-compose.dev.yml up -d
	@echo "Development environment started!"
	@echo "  Application: http://localhost:8009"
	@echo "  Redis: localhost:6379"

dev-down: check-docker
	@echo "Stopping development environment..."
	sudo docker compose -f docker-compose.dev.yml down

dev-logs: check-docker
	sudo docker compose -f docker-compose.dev.yml logs -f

# Maintenance targets
clean: check-docker
	@echo "Removing this project's containers, images, and volumes..."
	sudo docker compose down -v --rmi local --remove-orphans
	@echo "Nostorgraph containers, images, and volumes removed."

# Show .onion address
onion: check-docker
	@echo "Your .onion address:"
	@sudo docker exec -u debian-tor nostorgraph_app cat /var/lib/tor/hidden_service/hostname 2>/dev/null || echo "No .onion address found"
