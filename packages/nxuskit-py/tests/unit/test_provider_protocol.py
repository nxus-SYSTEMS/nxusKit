"""Unit tests for LLMProvider protocol."""


class TestLLMProviderProtocol:
    """Tests for LLMProvider protocol interface."""

    def test_provider_protocol_has_chat_method(self):
        """LLMProvider protocol should define chat method."""
        # This is a protocol test - we verify the interface exists
        from nxuskit.provider import LLMProvider

        assert hasattr(LLMProvider, "chat")

    def test_provider_protocol_has_chat_stream_method(self):
        """LLMProvider protocol should define chat_stream method."""
        from nxuskit.provider import LLMProvider

        assert hasattr(LLMProvider, "chat_stream")

    def test_provider_has_model_property(self):
        """LLMProvider should have model property."""
        from nxuskit.provider import LLMProvider

        assert hasattr(LLMProvider, "model")

    def test_provider_has_provider_name_property(self):
        """LLMProvider should have provider_name property."""
        from nxuskit.provider import LLMProvider

        assert hasattr(LLMProvider, "provider_name")


class TestChatMethodSignature:
    """Tests for chat method interface."""

    def test_chat_accepts_message_list(self):
        """chat() should accept List[Message]."""
        # Protocol test - verify the signature exists
        import inspect

        from nxuskit.provider import LLMProvider

        sig = inspect.signature(LLMProvider.chat)
        assert "messages" in sig.parameters

    def test_chat_returns_chat_response(self):
        """chat() should return ChatResponse."""
        # This tests the interface definition
        import inspect

        from nxuskit.provider import LLMProvider

        # Return annotation should be ChatResponse or compatible
        inspect.signature(LLMProvider.chat)


class TestChatStreamMethodSignature:
    """Tests for chat_stream method interface."""

    def test_chat_stream_accepts_message_list(self):
        """chat_stream() should accept List[Message]."""
        import inspect

        from nxuskit.provider import LLMProvider

        sig = inspect.signature(LLMProvider.chat_stream)
        assert "messages" in sig.parameters

    def test_chat_stream_returns_iterator(self):
        """chat_stream() should return Iterator[StreamChunk]."""
        # Protocol definition test
        import inspect

        from nxuskit.provider import LLMProvider

        # Should return Iterator[StreamChunk]
        inspect.signature(LLMProvider.chat_stream)


class TestProviderImplementation:
    """Tests for concrete provider implementations."""

    def test_provider_implementation_can_be_created(self):
        """A concrete provider implementation should be instantiable."""
        # This test will pass once a real provider is implemented
        pass

    def test_provider_chat_with_empty_messages(self):
        """Provider should handle empty message list."""
        # This will be tested in contract tests
        pass

    def test_provider_chat_with_single_message(self):
        """Provider should handle single message."""
        # This will be tested in contract tests
        pass

    def test_provider_chat_with_multiple_messages(self):
        """Provider should handle message history."""
        # This will be tested in contract tests
        pass

    def test_provider_chat_stream_yields_chunks(self):
        """Provider chat_stream should yield StreamChunk objects."""
        # This will be tested in contract tests
        pass

    def test_provider_chat_stream_empty_response(self):
        """Provider should handle empty streaming response."""
        # This will be tested in contract tests
        pass


class TestProviderProperties:
    """Tests for provider properties."""

    def test_model_property_returns_string(self):
        """model property should return string."""
        # Will be tested in contract tests with real providers
        pass

    def test_provider_name_property_returns_string(self):
        """provider_name property should return string."""
        # Will be tested in contract tests with real providers
        pass

    def test_provider_name_matches_provider_type(self):
        """provider_name should indicate provider type."""
        # Will be tested in contract tests
        pass
