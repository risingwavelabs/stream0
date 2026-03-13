package main

import (
	"context"
	"crypto/subtle"
	"net/http"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/gorilla/websocket"
	"github.com/sirupsen/logrus"
)

// Server handles HTTP requests
type Server struct {
	db       *Database
	router   *gin.Engine
	server   *http.Server
	upgrader websocket.Upgrader
}

// ProduceRequest represents a produce message request
type ProduceRequest struct {
	Payload map[string]interface{} `json:"payload" binding:"required"`
	Headers map[string]string      `json:"headers"`
	Key     string                 `json:"key"`
}

// ProduceResponse represents a produce message response
type ProduceResponse struct {
	MessageID string    `json:"message_id"`
	Offset    int64     `json:"offset"`
	Timestamp time.Time `json:"timestamp"`
}

// AckRequest represents an acknowledge request
type AckRequest struct {
	Group string `json:"group" binding:"required"`
}

// TopicCreateRequest represents a create topic request
type TopicCreateRequest struct {
	Name          string `json:"name" binding:"required"`
	RetentionDays int    `json:"retention_days"`
}

// NewServer creates a new server
func NewServer(db *Database, cfg ServerConfig, authCfg AuthConfig) *Server {
	gin.SetMode(gin.ReleaseMode)
	r := gin.New()

	// Structured logging middleware
	r.Use(ginLogger())
	r.Use(gin.Recovery())
	r.Use(corsMiddleware())

	s := &Server{
		db:     db,
		router: r,
		upgrader: websocket.Upgrader{
			CheckOrigin: func(r *http.Request) bool {
				return true
			},
		},
	}

	s.setupRoutes(authCfg)
	return s
}

func corsMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		c.Writer.Header().Set("Access-Control-Allow-Origin", "*")
		c.Writer.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
		c.Writer.Header().Set("Access-Control-Allow-Headers", "Content-Type, Authorization")

		if c.Request.Method == "OPTIONS" {
			c.AbortWithStatus(204)
			return
		}

		c.Next()
	}
}

func (s *Server) setupRoutes(authCfg AuthConfig) {
	// Public routes
	s.router.GET("/health", s.healthHandler)

	// Protected routes
	api := s.router.Group("/")
	if len(authCfg.APIKeys) > 0 {
		api.Use(apiKeyAuth(authCfg.APIKeys))
	}
	api.GET("/topics", s.listTopicsHandler)
	api.POST("/topics", s.createTopicHandler)
	api.GET("/topics/:topic", s.getTopicHandler)
	api.POST("/topics/:topic/messages", s.produceMessageHandler)
	api.GET("/topics/:topic/messages", s.consumeMessagesHandler)
	api.POST("/messages/:message_id/ack", s.acknowledgeMessageHandler)
	api.GET("/topics/:topic/subscribe", s.websocketHandler)
}

// apiKeyAuth returns middleware that validates the X-API-Key header
func apiKeyAuth(validKeys []string) gin.HandlerFunc {
	return func(c *gin.Context) {
		key := c.GetHeader("X-API-Key")
		if key == "" {
			c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "missing X-API-Key header"})
			return
		}

		for _, valid := range validKeys {
			if subtle.ConstantTimeCompare([]byte(key), []byte(valid)) == 1 {
				c.Next()
				return
			}
		}

		c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "invalid API key"})
	}
}

// Run starts the server
func (s *Server) Run(addr string) error {
	s.server = &http.Server{
		Addr:    addr,
		Handler: s.router,
	}
	return s.server.ListenAndServe()
}

// Shutdown gracefully shuts down the server
func (s *Server) Shutdown(ctx context.Context) error {
	if s.server != nil {
		return s.server.Shutdown(ctx)
	}
	return nil
}

// ginLogger returns a Gin middleware for structured logging
func ginLogger() gin.HandlerFunc {
	return func(c *gin.Context) {
		start := time.Now()
		path := c.Request.URL.Path
		raw := c.Request.URL.RawQuery

		// Process request
		c.Next()

		// Log after request
		timestamp := time.Now()
		latency := timestamp.Sub(start)

		clientIP := c.ClientIP()
		method := c.Request.Method
		statusCode := c.Writer.Status()

		if raw != "" {
			path = path + "?" + raw
		}

		fields := logrus.Fields{
			"client_ip":  clientIP,
			"timestamp":  timestamp.Format(time.RFC3339),
			"method":     method,
			"path":       path,
			"status":     statusCode,
			"latency":    latency,
			"user_agent": c.Request.UserAgent(),
		}

		entry := logrus.WithFields(fields)

		if statusCode >= 500 {
			entry.Error("Server error")
		} else if statusCode >= 400 {
			entry.Warn("Client error")
		} else {
			entry.Info("Request")
		}
	}
}

func (s *Server) healthHandler(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status":  "healthy",
		"version": "0.1.0-go",
	})
}

func (s *Server) listTopicsHandler(c *gin.Context) {
	topics, err := s.db.ListTopics()
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, topics)
}

func (s *Server) createTopicHandler(c *gin.Context) {
	var req TopicCreateRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	if req.RetentionDays == 0 {
		req.RetentionDays = 7
	}

	topic, err := s.db.CreateTopic(req.Name, req.RetentionDays)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusCreated, topic)
}

func (s *Server) getTopicHandler(c *gin.Context) {
	topicName := c.Param("topic")
	topic, err := s.db.GetTopic(topicName)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	if topic == nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Topic not found"})
		return
	}

	c.JSON(http.StatusOK, topic)
}

func (s *Server) produceMessageHandler(c *gin.Context) {
	topicName := c.Param("topic")
	topic, err := s.db.GetTopic(topicName)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	if topic == nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Topic not found"})
		return
	}

	var req ProduceRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	if req.Headers == nil {
		req.Headers = make(map[string]string)
	}

	msg, err := s.db.PublishMessage(topic.ID, req.Payload, req.Headers)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusCreated, ProduceResponse{
		MessageID: msg.ID,
		Offset:    msg.Offset,
		Timestamp: msg.Timestamp,
	})
}

func (s *Server) consumeMessagesHandler(c *gin.Context) {
	topicName := c.Param("topic")
	topic, err := s.db.GetTopic(topicName)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	if topic == nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Topic not found"})
		return
	}

	group := c.Query("group")
	if group == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "group is required"})
		return
	}

	max, _ := strconv.Atoi(c.DefaultQuery("max", "10"))
	if max < 1 {
		max = 10
	}
	if max > 100 {
		max = 100
	}

	timeout, _ := strconv.ParseFloat(c.DefaultQuery("timeout", "5"), 64)
	if timeout < 0 {
		timeout = 5
	}
	if timeout > 30 {
		timeout = 30
	}

	visibilityTimeout, _ := strconv.Atoi(c.DefaultQuery("visibility_timeout", "30"))
	if visibilityTimeout < 5 {
		visibilityTimeout = 5
	}
	if visibilityTimeout > 300 {
		visibilityTimeout = 300
	}

	consumerID := generateConsumerID()

	// Long polling
	start := time.Now()
	pollInterval := 500 * time.Millisecond

	for {
		messages, err := s.db.ClaimMessages(topic.ID, group, consumerID, max, visibilityTimeout)
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}

		if len(messages) > 0 {
			c.JSON(http.StatusOK, gin.H{"messages": messages})
			return
		}

		elapsed := time.Since(start).Seconds()
		if elapsed >= timeout {
			c.JSON(http.StatusOK, gin.H{"messages": []Message{}})
			return
		}

		time.Sleep(pollInterval)
	}
}

func (s *Server) acknowledgeMessageHandler(c *gin.Context) {
	messageID := c.Param("message_id")

	var req AckRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	err := s.db.AcknowledgeMessage(messageID, req.Group)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"status":     "acknowledged",
		"message_id": messageID,
	})
}

func (s *Server) websocketHandler(c *gin.Context) {
	// Upgrade to WebSocket
	conn, err := s.upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		return
	}
	defer conn.Close()

	topicName := c.Param("topic")
	topic, err := s.db.GetTopic(topicName)
	if err != nil || topic == nil {
		conn.WriteJSON(gin.H{"type": "error", "message": "Topic not found"})
		return
	}

	group := c.Query("group")
	if group == "" {
		group = "default"
	}

	visibilityTimeout, _ := strconv.Atoi(c.DefaultQuery("visibility_timeout", "30"))
	if visibilityTimeout < 5 {
		visibilityTimeout = 5
	}

	consumerID := generateConsumerID()

	// WebSocket message handling
	for {
		// Try to claim messages
		messages, err := s.db.ClaimMessages(topic.ID, group, consumerID, 10, visibilityTimeout)
		if err != nil {
			conn.WriteJSON(gin.H{"type": "error", "message": err.Error()})
			return
		}

		for _, msg := range messages {
			if err := conn.WriteJSON(gin.H{
				"type":           "message",
				"id":             msg.ID,
				"payload":        msg.Payload,
				"headers":        msg.Headers,
				"delivery_count": msg.DeliveryCount,
			}); err != nil {
				return
			}
		}

		// Handle client messages (acks)
		conn.SetReadDeadline(time.Now().Add(100 * time.Millisecond))
		var clientMsg map[string]interface{}
		if err := conn.ReadJSON(&clientMsg); err == nil {
			action, _ := clientMsg["action"].(string)
			if action == "ack" {
				msgID, _ := clientMsg["message_id"].(string)
				s.db.AcknowledgeMessage(msgID, group)
			}
		}

		time.Sleep(100 * time.Millisecond)
	}
}

func generateConsumerID() string {
	return "consumer-" + strconv.FormatInt(time.Now().UnixNano(), 36)
}
